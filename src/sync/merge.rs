//! Three-way merge of two snapshots of a store against their common
//! ancestor, entity by entity (tasks and buckets, by uuid).
//!
//! | Situation | Result |
//! |---|---|
//! | changed on one side | that side |
//! | changed on both | field by field; a field changed on both → the more recent edit wins |
//! | created on one side | kept |
//! | deleted on one side, untouched on the other | deleted |
//! | deleted on one side, edited on the other | the more recent of the delete and the edit wins |
//! | bucket deleted, but holds tasks kept by the merge | the bucket is kept |
//! | two buckets with the same name | merged into the more recently edited one |
//!
//! Entities are compared as JSON objects, so fields added to tasks later
//! are merged without changes here. `status` and `time_completed` are
//! merged together, so a task can't end up open with a completion time.

use std::collections::{BTreeMap, BTreeSet};

use jiff::Timestamp;
use serde_json::{Map, Value, json};
use uuid::Uuid;

use super::Snapshot;
use crate::bucket::Bucket;
use crate::store::Tombstones;
use crate::task::Task;
use crate::utils::header::Header;

/// Task fields that only make sense together, merged as one.
const COUPLED: &[&[&str]] = &[&["status", "time_completed"]];

/// The result of a merge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Merged {
    pub snapshot: Snapshot,
    /// Every decision made between conflicting changes, for reporting.
    pub conflicts: Vec<Conflict>,
    /// Buckets merged into another bucket: `merged-away id → survivor id`.
    pub redirects: BTreeMap<Uuid, Uuid>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Task,
    Bucket,
}

/// A decision between changes that couldn't both be kept.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conflict {
    pub kind: Kind,
    pub id: Uuid,
    pub what: What,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum What {
    /// The field `key` was changed on both sides; `kept` was edited later.
    /// The values are as kru displays them.
    Field {
        key: String,
        kept: String,
        dropped: String,
    },
    /// Edited on one side after it was deleted on the other: kept.
    EditBeatDelete,
    /// Deleted on one side after it was edited on the other: deleted.
    DeleteBeatEdit,
    /// A bucket deleted on one side still held tasks the merge kept.
    BucketKept,
    /// A bucket with the same name as another was merged into it.
    BucketsMerged { into: Uuid },
    /// A task's notes file changed in the same places on both devices; the
    /// more recently committed one was kept.
    NotesOverwritten { kept_this_device: bool },
}

impl Conflict {
    /// A short description, e.g. `priority: kept high over medium`.
    pub fn summary(&self) -> String {
        match &self.what {
            What::Field { key, kept, dropped } => format!("{key}: kept {kept} over {dropped}"),
            What::EditBeatDelete => "kept: edited after it was deleted elsewhere".into(),
            What::DeleteBeatEdit => "deleted: deleted after it was edited elsewhere".into(),
            What::BucketKept => "bucket kept: it still has tasks".into(),
            What::BucketsMerged { .. } => {
                "merged with the bucket of the same name from the other device".into()
            }
            What::NotesOverwritten { kept_this_device } => format!(
                "notes: edited in the same places on both devices; kept {}",
                if *kept_this_device {
                    "this device's (newer)"
                } else {
                    "the other device's (newer)"
                }
            ),
        }
    }
}

/// Merges `ours` and `theirs`, two versions of `base`. Pass an empty base
/// when they have no common ancestor. The result is the same whichever
/// side is "ours".
pub fn merge(base: &Snapshot, ours: &Snapshot, theirs: &Snapshot) -> Merged {
    let mut conflicts = Vec::new();

    let buckets = merge_all(
        Kind::Bucket,
        [bucket_maps(base), bucket_maps(ours), bucket_maps(theirs)],
        [&ours.tombstones.buckets, &theirs.tombstones.buckets],
        &mut conflicts,
    );
    let tasks = merge_all(
        Kind::Task,
        [task_maps(base), task_maps(ours), task_maps(theirs)],
        [&ours.tombstones.tasks, &theirs.tombstones.tasks],
        &mut conflicts,
    );

    let mut tombstones = ours.tombstones.clone();
    tombstones.absorb(&theirs.tombstones);

    // Rebuild buckets, then put the tasks in them.
    let mut result: BTreeMap<Uuid, Bucket> = buckets
        .into_iter()
        .map(|(id, map)| (id, bucket_from(map)))
        .collect();
    for mut map in tasks.into_values() {
        let bucket_id = map
            .remove("bucket")
            .and_then(|v| v.as_str().and_then(|s| s.parse::<Uuid>().ok()));
        let task: Task =
            serde_json::from_value(Value::Object(map)).expect("merged from valid tasks");
        let bucket_id = bucket_id.unwrap_or(task.header.id);
        let bucket = result.entry(bucket_id).or_insert_with(|| {
            // Deleted by the merge, but this task outlived it: keep it.
            conflicts.push(Conflict {
                kind: Kind::Bucket,
                id: bucket_id,
                what: What::BucketKept,
            });
            [ours, theirs, base]
                .iter()
                .filter_map(|side| side.buckets.get(&bucket_id))
                .max_by_key(|b| b.header.time_last_edited)
                .map(meta_only)
                .unwrap_or_else(|| Bucket::new("Recovered"))
        });
        bucket.add(task);
    }

    let redirects = merge_same_names(&mut result, &mut tombstones, &mut conflicts);

    // What exists isn't deleted.
    let task_ids: BTreeSet<Uuid> = result
        .values()
        .flat_map(|b| b.iter().map(|t| t.header.id))
        .collect();
    tombstones.tasks.retain(|id, _| !task_ids.contains(id));
    tombstones.buckets.retain(|id, _| !result.contains_key(id));

    Merged {
        snapshot: Snapshot {
            buckets: result,
            tombstones,
        },
        conflicts,
        redirects,
    }
}

/// Merges every entity of one kind. `maps` are base, ours, theirs; `stones`
/// are ours' and theirs' tombstones for that kind.
fn merge_all(
    kind: Kind,
    maps: [BTreeMap<Uuid, Map<String, Value>>; 3],
    stones: [&BTreeMap<Uuid, Timestamp>; 2],
    conflicts: &mut Vec<Conflict>,
) -> BTreeMap<Uuid, Map<String, Value>> {
    let [base, ours, theirs] = maps;
    let ids: BTreeSet<Uuid> = base
        .keys()
        .chain(ours.keys())
        .chain(theirs.keys())
        .copied()
        .collect();

    let mut merged = BTreeMap::new();
    for id in ids {
        let base = base.get(&id);
        let kept = match (ours.get(&id), theirs.get(&id)) {
            (Some(o), Some(t)) if o == t => Some(o.clone()),
            (Some(o), Some(t)) => Some(merge_fields(kind, id, base, o, t, conflicts)),
            (Some(o), None) => one_sided(kind, id, o, base, stones[1].get(&id), conflicts),
            (None, Some(t)) => one_sided(kind, id, t, base, stones[0].get(&id), conflicts),
            (None, None) => None,
        };
        if let Some(map) = kept {
            merged.insert(id, map);
        }
    }
    merged
}

/// An entity present on one side only: created there, or deleted on the
/// other side (`deleted_at`, if that side recorded when).
fn one_sided(
    kind: Kind,
    id: Uuid,
    present: &Map<String, Value>,
    base: Option<&Map<String, Value>>,
    deleted_at: Option<&Timestamp>,
    conflicts: &mut Vec<Conflict>,
) -> Option<Map<String, Value>> {
    let untouched = base == Some(present);
    let decision = match deleted_at {
        // The later of the delete and the edit wins; an edit wins a tie.
        Some(&at) if at > edited(present) => None,
        Some(_) => Some(present.clone()),
        // Deleted without a record of when: delete only if untouched here.
        None if untouched => None,
        None => Some(present.clone()),
    };
    let deleted_elsewhere = deleted_at.is_some() || base.is_some();
    if deleted_elsewhere && !untouched {
        conflicts.push(Conflict {
            kind,
            id,
            what: if decision.is_some() {
                What::EditBeatDelete
            } else {
                What::DeleteBeatEdit
            },
        });
    }
    decision
}

/// Merges an entity changed on both sides, field by field. The header
/// keeps the later edit time.
fn merge_fields(
    kind: Kind,
    id: Uuid,
    base: Option<&Map<String, Value>>,
    ours: &Map<String, Value>,
    theirs: &Map<String, Value>,
    conflicts: &mut Vec<Conflict>,
) -> Map<String, Value> {
    let empty = Map::new();
    let base = base.unwrap_or(&empty);
    let (ours_at, theirs_at) = (edited(ours), edited(theirs));

    let mut keys: Vec<&String> = ours.keys().collect();
    keys.extend(theirs.keys().filter(|k| !ours.contains_key(*k)));

    let mut merged = Map::new();
    let mut done: BTreeSet<&str> = BTreeSet::new();
    for key in keys {
        if key == "header" || done.contains(key.as_str()) {
            continue;
        }
        let group: Vec<&str> = COUPLED
            .iter()
            .find(|group| group.contains(&key.as_str()))
            .map(|group| group.to_vec())
            .unwrap_or_else(|| vec![key.as_str()]);
        done.extend(group.iter().copied());

        let pick = |side: &Map<String, Value>| -> Vec<Option<Value>> {
            group.iter().map(|k| side.get(*k).cloned()).collect()
        };
        let (o, t, b) = (pick(ours), pick(theirs), pick(base));
        let take_ours = if o == t || t == b {
            true
        } else if o == b {
            false
        } else {
            // Changed on both sides: the later edit wins; on a tie, the
            // larger value, so both devices decide the same way.
            let ours_wins = match ours_at.cmp(&theirs_at) {
                std::cmp::Ordering::Equal => {
                    serde_json::to_string(&o).ok() >= serde_json::to_string(&t).ok()
                }
                order => order.is_gt(),
            };
            let (kept, dropped) = if ours_wins {
                (ours, theirs)
            } else {
                (theirs, ours)
            };
            conflicts.push(Conflict {
                kind,
                id,
                what: What::Field {
                    key: group[0].to_string(),
                    kept: displayed(kind, kept, group[0]),
                    dropped: displayed(kind, dropped, group[0]),
                },
            });
            ours_wins
        };
        for (k, value) in group.iter().zip(if take_ours { o } else { t }) {
            if let Some(value) = value {
                merged.insert(k.to_string(), value);
            }
        }
    }

    let mut header = ours["header"].clone();
    if theirs_at > ours_at {
        header["time_last_edited"] = theirs["header"]["time_last_edited"].clone();
    }
    let mut result = Map::new();
    result.insert("header".into(), header);
    result.extend(merged);
    result
}

/// Merges buckets that share a name (case-insensitive) into the most
/// recently edited of them, which keeps its id and name. Returns which
/// buckets were merged into which.
fn merge_same_names(
    buckets: &mut BTreeMap<Uuid, Bucket>,
    tombstones: &mut Tombstones,
    conflicts: &mut Vec<Conflict>,
) -> BTreeMap<Uuid, Uuid> {
    let mut by_name: BTreeMap<String, Vec<Uuid>> = BTreeMap::new();
    for (id, bucket) in buckets.iter() {
        by_name
            .entry(bucket.name.to_lowercase())
            .or_default()
            .push(*id);
    }

    let mut redirects = BTreeMap::new();
    for ids in by_name.into_values().filter(|ids| ids.len() > 1) {
        let survivor = *ids
            .iter()
            .max_by_key(|id| (buckets[id].header.time_last_edited, **id))
            .expect("at least two");
        for id in ids.into_iter().filter(|id| *id != survivor) {
            let merged_away = buckets.remove(&id).expect("listed above");
            let target = buckets.get_mut(&survivor).expect("still there");
            for task in merged_away.iter() {
                target.add(task.clone());
            }
            tombstones.buckets.insert(id, Timestamp::now());
            redirects.insert(id, survivor);
            conflicts.push(Conflict {
                kind: Kind::Bucket,
                id,
                what: What::BucketsMerged { into: survivor },
            });
        }
    }
    redirects
}

// --- Entities as JSON ---------------------------------------------------

fn bucket_maps(snapshot: &Snapshot) -> BTreeMap<Uuid, Map<String, Value>> {
    snapshot
        .buckets
        .iter()
        .map(|(id, bucket)| {
            let map = json!({ "header": bucket.header, "name": bucket.name });
            (*id, into_map(map))
        })
        .collect()
}

/// Tasks keyed by id, each with a `bucket` key, so a move is a field change.
fn task_maps(snapshot: &Snapshot) -> BTreeMap<Uuid, Map<String, Value>> {
    let mut maps = BTreeMap::new();
    for (bucket_id, bucket) in &snapshot.buckets {
        for task in bucket.iter() {
            let mut map = into_map(serde_json::to_value(task).expect("tasks serialize"));
            map.insert("bucket".into(), json!(bucket_id));
            maps.insert(task.header.id, map);
        }
    }
    maps
}

fn bucket_from(map: Map<String, Value>) -> Bucket {
    let header: Header =
        serde_json::from_value(map["header"].clone()).expect("merged from valid buckets");
    let mut bucket = Bucket::new(map["name"].as_str().unwrap_or_default());
    bucket.header = header;
    bucket
}

/// A bucket's name and header without its tasks.
fn meta_only(bucket: &Bucket) -> Bucket {
    let mut copy = Bucket::new(bucket.name.clone());
    copy.header = bucket.header;
    copy
}

fn into_map(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        _ => Map::new(),
    }
}

/// When an entity was last edited.
fn edited(map: &Map<String, Value>) -> Timestamp {
    map.get("header")
        .and_then(|h| h.get("time_last_edited"))
        .and_then(Value::as_str)
        .and_then(|s| s.parse().ok())
        .unwrap_or(Timestamp::MIN)
}

/// A field's value as kru displays it (`medium`, a formatted date…),
/// falling back to the stored JSON for anything that isn't a task field.
fn displayed(kind: Kind, entity: &Map<String, Value>, key: &str) -> String {
    let field = (kind == Kind::Task)
        .then(|| serde_json::from_value::<Task>(Value::Object(entity.clone())).ok())
        .flatten()
        .and_then(|task| task.fields().into_iter().find(|f| f.key == key));
    match field {
        Some(field) => {
            let text = field.value.short();
            if text.is_empty() { "—".into() } else { text }
        }
        None => shown(entity.get(key).unwrap_or(&Value::Null)),
    }
}

fn shown(value: &Value) -> String {
    match value {
        Value::Null => "—".into(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::{Priority, Status};

    /// A point in time, `n` minutes into a fixed day.
    fn at(n: i64) -> Timestamp {
        Timestamp::from_second(1_800_000_000 + n * 60).unwrap()
    }

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn bucket(n: u128, name: &str, edited: i64) -> Bucket {
        let mut bucket = Bucket::new(name);
        bucket.header.id = id(n);
        bucket.header.time_created = at(0);
        bucket.header.time_last_edited = at(edited);
        bucket
    }

    fn task(n: u128, description: &str, edited: i64) -> Task {
        let mut task = Task::new(description);
        task.header.id = id(n);
        task.header.time_created = at(0);
        task.header.time_last_edited = at(edited);
        task
    }

    /// A snapshot with one bucket (id 100, "home") holding `tasks`.
    fn home(tasks: Vec<Task>) -> Snapshot {
        let mut home = bucket(100, "home", 0);
        for task in tasks {
            home.add(task);
        }
        Snapshot {
            buckets: BTreeMap::from([(home.header.id, home)]),
            tombstones: Tombstones::default(),
        }
    }

    fn get(snapshot: &Snapshot, n: u128) -> Option<&Task> {
        snapshot.buckets.values().find_map(|b| b.get(id(n)))
    }

    /// Merges both ways round and checks they agree.
    fn both_ways(base: &Snapshot, ours: &Snapshot, theirs: &Snapshot) -> Merged {
        let one = merge(base, ours, theirs);
        let other = merge(base, theirs, ours);
        assert_eq!(
            one.snapshot.buckets, other.snapshot.buckets,
            "merge must be symmetric"
        );
        one
    }

    #[test]
    fn changes_to_different_tasks_both_land() {
        let base = home(vec![task(1, "a", 0), task(2, "b", 0)]);
        let ours = home(vec![task(1, "a, edited here", 5), task(2, "b", 0)]);
        let theirs = home(vec![task(1, "a", 0), task(2, "b, edited there", 6)]);
        let merged = both_ways(&base, &ours, &theirs);
        assert_eq!(
            get(&merged.snapshot, 1).unwrap().description,
            "a, edited here"
        );
        assert_eq!(
            get(&merged.snapshot, 2).unwrap().description,
            "b, edited there"
        );
        assert!(merged.conflicts.is_empty());
    }

    #[test]
    fn different_fields_of_one_task_both_land() {
        let base = home(vec![task(1, "a", 0)]);
        let ours = home(vec![task(1, "a, renamed", 5)]);
        let mut high = task(1, "a", 7);
        high.priority = Priority::High;
        let theirs = home(vec![high]);
        let merged = both_ways(&base, &ours, &theirs);
        let result = get(&merged.snapshot, 1).unwrap();
        assert_eq!(result.description, "a, renamed");
        assert_eq!(result.priority, Priority::High);
        assert_eq!(
            result.header.time_last_edited,
            at(7),
            "keeps the later edit time"
        );
        assert!(merged.conflicts.is_empty());
    }

    #[test]
    fn the_same_field_goes_to_the_later_edit() {
        let base = home(vec![task(1, "a", 0)]);
        let ours = home(vec![task(1, "ours", 9)]);
        let theirs = home(vec![task(1, "theirs", 4)]);
        let merged = both_ways(&base, &ours, &theirs);
        assert_eq!(get(&merged.snapshot, 1).unwrap().description, "ours");
        assert_eq!(merged.conflicts.len(), 1);
        assert_eq!(
            merged.conflicts[0].summary(),
            "description: kept ours over theirs"
        );
    }

    #[test]
    fn status_and_completion_stay_together() {
        let base = home(vec![task(1, "a", 0)]);
        let mut done = task(1, "a", 5);
        done.status = Status::Done;
        done.time_completed = Some(at(5));
        let mut doing = task(1, "a", 8);
        doing.status = Status::InProgress;
        let merged = both_ways(&base, &home(vec![done]), &home(vec![doing]));
        let result = get(&merged.snapshot, 1).unwrap();
        assert_eq!(result.status, Status::InProgress);
        assert_eq!(
            result.time_completed, None,
            "no completion time on an open task"
        );
    }

    #[test]
    fn new_tasks_from_both_sides_are_kept() {
        let base = home(vec![]);
        let merged = both_ways(
            &base,
            &home(vec![task(1, "a", 1)]),
            &home(vec![task(2, "b", 2)]),
        );
        assert!(get(&merged.snapshot, 1).is_some() && get(&merged.snapshot, 2).is_some());
    }

    #[test]
    fn a_delete_wins_over_an_untouched_task() {
        let base = home(vec![task(1, "a", 0)]);
        let mut theirs = home(vec![]);
        theirs.tombstones.tasks.insert(id(1), at(3));
        let merged = both_ways(&base, &base.clone(), &theirs);
        assert!(get(&merged.snapshot, 1).is_none());
        assert_eq!(merged.snapshot.tombstones.tasks.get(&id(1)), Some(&at(3)));
        assert!(merged.conflicts.is_empty());

        // Also without a tombstone (e.g. one that expired).
        let merged = both_ways(&base, &base.clone(), &home(vec![]));
        assert!(get(&merged.snapshot, 1).is_none());
    }

    #[test]
    fn the_later_of_a_delete_and_an_edit_wins() {
        let base = home(vec![task(1, "a", 0)]);
        let edited = home(vec![task(1, "a, edited", 5)]);

        let mut deleted_earlier = home(vec![]);
        deleted_earlier.tombstones.tasks.insert(id(1), at(3));
        let merged = both_ways(&base, &edited, &deleted_earlier);
        assert_eq!(get(&merged.snapshot, 1).unwrap().description, "a, edited");
        assert!(
            merged.snapshot.tombstones.tasks.is_empty(),
            "it exists again"
        );
        assert_eq!(merged.conflicts[0].what, What::EditBeatDelete);

        let mut deleted_later = home(vec![]);
        deleted_later.tombstones.tasks.insert(id(1), at(8));
        let merged = both_ways(&base, &edited, &deleted_later);
        assert!(get(&merged.snapshot, 1).is_none());
        assert_eq!(merged.conflicts[0].what, What::DeleteBeatEdit);
    }

    #[test]
    fn a_move_and_an_edit_both_land() {
        let mut base = home(vec![task(1, "a", 0)]);
        let work = bucket(200, "work", 0);
        base.buckets.insert(work.header.id, work.clone());

        // Ours moves the task to work; theirs edits it in home.
        let mut ours = base.clone();
        let moved = ours
            .buckets
            .get_mut(&id(100))
            .unwrap()
            .remove(id(1))
            .unwrap();
        let mut moved = moved;
        moved.header.time_last_edited = at(4);
        ours.buckets.get_mut(&id(200)).unwrap().add(moved);
        let mut theirs = base.clone();
        theirs
            .buckets
            .get_mut(&id(100))
            .unwrap()
            .get_mut(id(1))
            .unwrap()
            .description = "a, edited".into();

        let merged = both_ways(&base, &ours, &theirs);
        let in_work = merged.snapshot.buckets[&id(200)].get(id(1)).unwrap();
        assert_eq!(in_work.description, "a, edited");
        assert!(merged.snapshot.buckets[&id(100)].get(id(1)).is_none());
    }

    #[test]
    fn a_deleted_bucket_is_kept_for_a_newer_task() {
        let base = home(vec![task(1, "a", 0)]);
        let mut deleted = Snapshot::default();
        deleted.tombstones.buckets.insert(id(100), at(3));
        deleted.tombstones.tasks.insert(id(1), at(3));
        // The other device added a task to the bucket after that.
        let added = home(vec![task(1, "a", 0), task(2, "b", 6)]);

        let merged = both_ways(&base, &deleted, &added);
        assert!(
            merged.snapshot.buckets.contains_key(&id(100)),
            "the bucket is kept"
        );
        assert!(
            get(&merged.snapshot, 2).is_some(),
            "the newer task survives"
        );
        assert!(
            get(&merged.snapshot, 1).is_none(),
            "the untouched task stays deleted"
        );
        assert!(merged.conflicts.iter().any(|c| c.what == What::BucketKept));
    }

    #[test]
    fn same_name_buckets_merge_into_the_later_edited_one() {
        let mut ours = Snapshot::default();
        let mut mine = bucket(100, "Home", 2);
        mine.add(task(1, "a", 1));
        ours.buckets.insert(mine.header.id, mine);
        let mut theirs = Snapshot::default();
        let mut other = bucket(200, "home", 5);
        other.add(task(2, "b", 1));
        theirs.buckets.insert(other.header.id, other);

        // No common ancestor: two stores synced for the first time.
        let merged = both_ways(&Snapshot::default(), &ours, &theirs);
        assert_eq!(merged.snapshot.buckets.len(), 1);
        let survivor = &merged.snapshot.buckets[&id(200)];
        assert_eq!(survivor.name, "home");
        assert_eq!(survivor.len(), 2);
        assert_eq!(merged.redirects.get(&id(100)), Some(&id(200)));
        assert!(merged.snapshot.tombstones.buckets.contains_key(&id(100)));
    }
}

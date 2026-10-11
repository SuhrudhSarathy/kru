use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use jiff::{SignedDuration, Timestamp};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use uuid::Uuid;

use super::migrate::{self, File};
use super::{DONE_RETENTION, State, Store, TOMBSTONE_RETENTION, Tombstones};
use crate::bucket::Bucket;
use crate::sync::{self, Snapshot};
use crate::utils::error::{Error, Result};

impl Store {
    /// Opens the store in [`Store::default_dir`].
    pub fn open_default() -> Result<Store> {
        Store::open(Store::default_dir()?)
    }

    /// Loads the store from `dir`, creating it if needed, and prunes done
    /// tasks older than [`DONE_RETENTION`] (and old tombstones).
    pub fn open(dir: impl Into<PathBuf>) -> Result<Store> {
        let dir = dir.into();
        let (snapshot, mut state) = {
            let _lock = lock(&dir)?;
            let state_path = dir.join("state.json");
            let state: State = if state_path.exists() {
                read_versioned(&state_path, File::State)?
            } else {
                State::default()
            };
            (load_snapshot(&dir)?, state)
        };

        // If the active bucket has somehow been deleted, reset it to none.
        if state
            .active
            .is_some_and(|id| !snapshot.buckets.contains_key(&id))
        {
            state.active = None;
        }

        let mut store = Store {
            dir,
            buckets: snapshot.buckets.clone(),
            state,
            tombstones: snapshot.tombstones.clone(),
            base: snapshot,
        };

        let pruned = store.prune(DONE_RETENTION) > 0;
        let expired = store.tombstones.expire(TOMBSTONE_RETENTION);
        if pruned || expired {
            store.save()?;
        }
        Ok(store)
    }

    /// Writes the store to disk. If another process (a CLI command, a sync,
    /// another window) changed the files since this store was loaded, its
    /// changes are merged in first (see [`crate::sync::merge`]) and this
    /// store is updated to the merged result, so nobody's edits are lost.
    pub fn save(&mut self) -> Result<()> {
        let _lock = lock(&self.dir)?;
        let on_disk = load_snapshot(&self.dir)?;
        if on_disk != self.base {
            let merged = sync::merge(&self.base, &self.snapshot(), &on_disk);
            self.buckets = merged.snapshot.buckets;
            self.tombstones = merged.snapshot.tombstones;
            self.follow_redirects(&merged.redirects);
        }

        let snapshot = self.snapshot();
        write_snapshot(&self.dir, &snapshot)?;
        write_json(&self.dir.join("state.json"), &self.state)?;
        self.base = snapshot;
        Ok(())
    }

    /// Points the active bucket at the bucket it was merged into, if any.
    pub(crate) fn follow_redirects(&mut self, redirects: &BTreeMap<Uuid, Uuid>) {
        if let Some(active) = self.state.active
            && let Some(&survivor) = redirects.get(&active)
        {
            self.state.active = Some(survivor);
        }
        if self
            .state
            .active
            .is_some_and(|id| !self.buckets.contains_key(&id))
        {
            self.state.active = None;
        }
    }

    /// Deletes done tasks completed more than `retention` ago, with their
    /// markdown files. Returns the number of tasks deleted.
    pub fn prune(&mut self, retention: SignedDuration) -> usize {
        let cutoff = Timestamp::now()
            .checked_sub(retention)
            .unwrap_or(Timestamp::MIN);
        let expired: Vec<Uuid> = self
            .entries()
            .filter(|e| {
                let completed = e
                    .task
                    .time_completed
                    .unwrap_or(e.task.header.time_last_edited);
                e.task.is_done() && completed < cutoff
            })
            .map(|e| e.task.header.id)
            .collect();
        for &id in &expired {
            let _ = self.remove_task(id);
        }
        expired.len()
    }
}

/// Holds an exclusive lock on the store directory until dropped, so loads
/// and saves from several processes don't interleave.
pub(crate) fn lock(dir: &Path) -> Result<fs::File> {
    fs::create_dir_all(dir).map_err(io_error(dir))?;
    let path = dir.join(".lock");
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .map_err(io_error(&path))?;
    file.lock().map_err(io_error(&path))?;
    Ok(file)
}

/// Reads the syncable part of the store in `dir` (buckets and tombstones).
pub(crate) fn load_snapshot(dir: &Path) -> Result<Snapshot> {
    let buckets_dir = dir.join("buckets");
    fs::create_dir_all(&buckets_dir).map_err(io_error(&buckets_dir))?;
    let mut buckets = BTreeMap::new();
    for path in json_files(&buckets_dir)? {
        let bucket: Bucket = read_versioned(&path, File::Bucket)?;
        buckets.insert(bucket.header.id, bucket);
    }
    let stones_path = dir.join("tombstones.json");
    let tombstones = if stones_path.exists() {
        read_versioned(&stones_path, File::Tombstones)?
    } else {
        Tombstones::default()
    };
    Ok(Snapshot {
        buckets,
        tombstones,
    })
}

/// Writes the syncable part of a store to `dir`: every bucket, the
/// tombstones (if any), and removes files of buckets that no longer exist.
pub(crate) fn write_snapshot(dir: &Path, snapshot: &Snapshot) -> Result<()> {
    let buckets_dir = dir.join("buckets");
    fs::create_dir_all(&buckets_dir).map_err(io_error(&buckets_dir))?;
    for bucket in snapshot.buckets.values() {
        write_json(&bucket_path(&buckets_dir, bucket.header.id), bucket)?;
    }
    for path in json_files(&buckets_dir)? {
        let id = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .and_then(|stem| stem.parse::<Uuid>().ok());
        if id.is_some_and(|id| !snapshot.buckets.contains_key(&id)) {
            fs::remove_file(&path).map_err(io_error(&path))?;
        }
    }
    let stones_path = dir.join("tombstones.json");
    if snapshot.tombstones.is_empty() {
        if stones_path.exists() {
            fs::remove_file(&stones_path).map_err(io_error(&stones_path))?;
        }
        Ok(())
    } else {
        write_json(&stones_path, &snapshot.tombstones)
    }
}

/// Path of a bucket's JSON file.
fn bucket_path(buckets_dir: &Path, id: Uuid) -> PathBuf {
    buckets_dir.join(format!("{id}.json"))
}

/// Builds a `map_err` closure that tags an I/O error with `path`.
fn io_error(path: &Path) -> impl FnOnce(std::io::Error) -> Error + '_ {
    move |source| Error::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// All `*.json` files directly inside `dir`.
pub(super) fn json_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in fs::read_dir(dir).map_err(io_error(dir))? {
        let path = entry.map_err(io_error(dir))?.path();
        if path.extension().is_some_and(|ext| ext == "json") {
            files.push(path);
        }
    }
    Ok(files)
}

/// Reads a store file, upgrading it first if an older kru wrote it (see
/// [`migrate`]), and deserializes it. Before upgrading, the original is
/// copied to `<file>.v<version>.bak`. Fails on files from a newer kru.
fn read_versioned<T: DeserializeOwned>(path: &Path, kind: File) -> Result<T> {
    let text = fs::read_to_string(path).map_err(io_error(path))?;
    let (value, version) = parse_versioned(&text, path, kind)?;
    let backup = path.with_extension(format!("json.v{version}.bak"));
    if version < migrate::VERSION && !backup.exists() {
        fs::copy(path, &backup).map_err(io_error(&backup))?;
    }
    Ok(value)
}

/// Parses the text of a store file (from disk or from git), upgrading it
/// to the current format. Returns the value and the version it was written
/// in. `path` names the file in errors. Fails on files from a newer kru.
pub(crate) fn parse_versioned<T: DeserializeOwned>(
    text: &str,
    path: &Path,
    kind: File,
) -> Result<(T, u64)> {
    let json_error = |source| Error::Json {
        path: path.to_path_buf(),
        source,
    };
    let mut file: Value = serde_json::from_str(text).map_err(json_error)?;
    let version = migrate::version(&file);
    if version > migrate::VERSION {
        return Err(Error::NewerFormat {
            path: path.to_path_buf(),
            version,
            supported: migrate::VERSION,
        });
    }
    migrate::upgrade(kind, &mut file, version);
    let value = serde_json::from_value(file).map_err(json_error)?;
    Ok((value, version))
}

/// After a sync, makes sure this device's active bucket exists: a bucket
/// merged into another is followed to it (`redirects`), and a device with
/// no active bucket (one that just joined) uses the oldest bucket.
pub(crate) fn settle_active(dir: &Path, redirects: &BTreeMap<Uuid, Uuid>) -> Result<()> {
    let path = dir.join("state.json");
    let mut state: State = if path.exists() {
        read_versioned(&path, File::State)?
    } else {
        State::default()
    };
    let buckets = load_snapshot(dir)?.buckets;
    let mut active = state.active.map(|id| *redirects.get(&id).unwrap_or(&id));
    if active.is_none_or(|id| !buckets.contains_key(&id)) {
        active = buckets.keys().next().copied();
    }
    if active != state.active {
        state.active = active;
        write_json(&path, &state)?;
    }
    Ok(())
}

/// Writes a store file stamped with the current format version. Writes to
/// a temporary file first and renames it, so a crash never leaves a
/// half-written file behind.
fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let json_error = |source| Error::Json {
        path: path.to_path_buf(),
        source,
    };
    let value = migrate::stamp(serde_json::to_value(value).map_err(json_error)?);
    let text = serde_json::to_string_pretty(&value).map_err(json_error)?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, text).map_err(io_error(&tmp))?;
    fs::rename(&tmp, path).map_err(io_error(path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::test_util::TempDir;
    use crate::task::{Priority, Status, Task};

    #[test]
    fn prune_removes_only_old_done_tasks() {
        let dir = TempDir::new();
        let mut store = Store::open(&dir.0).unwrap();
        store.add_bucket("home").unwrap();

        let mut old_done = Task::new("old done");
        old_done.status = Status::Done;
        old_done.time_completed = Some(Timestamp::now() - SignedDuration::from_hours(8 * 24));
        let mut recent_done = Task::new("recent done");
        recent_done.set_status(Status::Done);

        store.add_task(None, old_done).unwrap();
        store.add_task(None, recent_done).unwrap();
        store.add_task(None, Task::new("open")).unwrap();
        store.save().unwrap();

        // Opening prunes automatically.
        let store = Store::open(&dir.0).unwrap();
        let left: Vec<_> = store
            .entries()
            .map(|e| e.task.description.clone())
            .collect();
        assert_eq!(left, ["recent done", "open"]);
    }

    /// A bucket file as kru wrote it before files were versioned.
    const UNVERSIONED_BUCKET: &str = r#"{
      "header": {
        "id": "01a129b5-bf8c-7764-9558-d591db369ce8",
        "time_created": "2026-10-10T10:00:00Z",
        "time_last_edited": "2026-10-10T10:00:00Z"
      },
      "name": "home",
      "tasks": {
        "01a129b5-c000-7000-8000-000000000001": {
          "header": {
            "id": "01a129b5-c000-7000-8000-000000000001",
            "time_created": "2026-10-10T10:00:00Z",
            "time_last_edited": "2026-10-10T10:00:00Z"
          },
          "description": "Fix the sink",
          "priority": "High",
          "status": "ToDo",
          "due": null,
          "time_completed": null
        }
      }
    }"#;

    #[test]
    fn old_files_are_upgraded_and_backed_up() {
        let dir = TempDir::new();
        let buckets = dir.0.join("buckets");
        fs::create_dir_all(&buckets).unwrap();
        let path = buckets.join("01a129b5-bf8c-7764-9558-d591db369ce8.json");
        fs::write(&path, UNVERSIONED_BUCKET).unwrap();

        let mut store = Store::open(&dir.0).unwrap();
        let task = store.entry(1).unwrap().task;
        assert_eq!(
            (task.description.as_str(), task.ctx.as_str()),
            ("Fix the sink", "")
        );

        // The original is kept, untouched; the file is rewritten on save.
        let backup = path.with_extension("json.v0.bak");
        assert_eq!(fs::read_to_string(&backup).unwrap(), UNVERSIONED_BUCKET);
        store.save().unwrap();
        let saved: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(saved["version"], migrate::VERSION);
        assert_eq!(
            json_files(&buckets).unwrap().len(),
            1,
            "backups aren't loaded"
        );

        // Reopening the upgraded store works and finds the same task.
        let store = Store::open(&dir.0).unwrap();
        assert_eq!(store.entry(1).unwrap().task.description, "Fix the sink");
    }

    #[test]
    fn files_from_a_newer_kru_are_refused() {
        let dir = TempDir::new();
        let buckets = dir.0.join("buckets");
        fs::create_dir_all(&buckets).unwrap();
        let newer = UNVERSIONED_BUCKET.replacen('{', r#"{ "version": 999,"#, 1);
        fs::write(
            buckets.join("01a129b5-bf8c-7764-9558-d591db369ce8.json"),
            newer,
        )
        .unwrap();

        assert!(matches!(
            Store::open(&dir.0),
            Err(Error::NewerFormat { version: 999, .. })
        ));
    }

    #[test]
    fn saves_from_two_open_stores_keep_both_edits() {
        let dir = TempDir::new();
        let mut setup = Store::open(&dir.0).unwrap();
        setup.add_bucket("home").unwrap();
        let first = setup.add_task(None, Task::new("first")).unwrap();
        let second = setup.add_task(None, Task::new("second")).unwrap();
        setup.save().unwrap();

        // Like a TUI and a CLI command, both opened before either saves.
        let mut tui = Store::open(&dir.0).unwrap();
        let mut cli = Store::open(&dir.0).unwrap();
        tui.edit_task(first, &[("priority", "high")]).unwrap();
        cli.edit_task(second, &[("description", "second, renamed")])
            .unwrap();
        let added = cli.add_task(None, Task::new("third")).unwrap();
        cli.save().unwrap();
        tui.save().unwrap();

        // The second save merged in the first instead of overwriting it,
        // and the saving store now holds the merged result too.
        for store in [&tui, &Store::open(&dir.0).unwrap()] {
            assert_eq!(store.find(first).unwrap().task.priority, Priority::High);
            assert_eq!(
                store.find(second).unwrap().task.description,
                "second, renamed"
            );
            assert!(store.find(added).is_ok());
        }
    }

    #[test]
    fn deletes_leave_tombstones() {
        let dir = TempDir::new();
        let mut store = Store::open(&dir.0).unwrap();
        store.add_bucket("home").unwrap();
        store.add_bucket("work").unwrap();
        let task = store.add_task(None, Task::new("a")).unwrap();
        let in_work = store.add_task(Some("work"), Task::new("b")).unwrap();
        store.remove_task(task).unwrap();
        let work = store.bucket("work").unwrap().header.id;
        store.remove_bucket("work", true).unwrap();
        store.save().unwrap();

        let stones = Store::open(&dir.0).unwrap().snapshot().tombstones;
        assert!(stones.tasks.contains_key(&task));
        assert!(
            stones.tasks.contains_key(&in_work),
            "a bucket's tasks go with it"
        );
        assert!(stones.buckets.contains_key(&work));

        // A deleted task removed elsewhere stays deleted when this store saves.
        let mut other = Store::open(&dir.0).unwrap();
        other.add_task(None, Task::new("c")).unwrap();
        other.save().unwrap();
        assert!(Store::open(&dir.0).unwrap().find(task).is_err());
    }

    #[test]
    fn stale_active_bucket_is_cleared_on_open() {
        let dir = TempDir::new();
        let mut store = Store::open(&dir.0).unwrap();
        let id = store.add_bucket("home").unwrap().header.id;
        store.save().unwrap();

        // Delete the bucket file behind the store's back.
        fs::remove_file(bucket_path(&dir.0.join("buckets"), id)).unwrap();

        let store = Store::open(&dir.0).unwrap();
        assert!(store.active_bucket().is_none());
    }
}

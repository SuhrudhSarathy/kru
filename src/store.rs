//! Persistent storage of buckets and their tasks.
//!
//! Layout inside the store directory (the user cache dir by default,
//! overridable with `KRU_DIR`):
//!
//! ```text
//! state.json             # { "active": <bucket id> }
//! buckets/<id>.json      # one file per bucket, tasks included
//! ```
//!
//! Every frontend (CLI, TUI, ...) goes through [`Store`]; it is the only
//! place that adds, edits or removes buckets and tasks.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use jiff::{SignedDuration, Timestamp};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::bucket::Bucket;
use crate::error::{Error, Result};
use crate::task::{Task, TaskEdit};

/// How long done tasks are kept before [`Store::prune`] deletes them.
pub const DONE_RETENTION: SignedDuration = SignedDuration::from_hours(7 * 24);

/// Environment variable that overrides the store directory.
pub const DIR_ENV: &str = "KRU_DIR";

#[derive(Debug, Default, Serialize, Deserialize)]
struct State {
    active: Option<Uuid>,
}

/// A task together with its bucket and its user-facing number.
///
/// Numbers are assigned over all tasks (done ones included) in a fixed
/// order: buckets by creation, then tasks by creation. Filtering a listing
/// therefore never changes a task's number.
#[derive(Clone, Copy, Debug)]
pub struct Entry<'a> {
    pub num: usize,
    pub bucket: &'a Bucket,
    pub task: &'a Task,
}

#[derive(Debug)]
pub struct Store {
    dir: PathBuf,
    buckets: BTreeMap<Uuid, Bucket>,
    state: State,
}

impl Store {
    /// `$KRU_DIR` if set, otherwise the platform cache dir
    /// (e.g. `~/Library/Caches/kru` on macOS, `~/.cache/kru` on Linux).
    pub fn default_dir() -> Result<PathBuf> {
        if let Some(dir) = std::env::var_os(DIR_ENV) {
            return Ok(dir.into());
        }
        ProjectDirs::from("", "", "kru")
            .map(|dirs| dirs.cache_dir().to_path_buf())
            .ok_or(Error::NoCacheDir)
    }

    pub fn open_default() -> Result<Store> {
        Store::open(Store::default_dir()?)
    }

    /// Loads the store from `dir`, creating it if needed, and prunes done
    /// tasks older than [`DONE_RETENTION`].
    pub fn open(dir: impl Into<PathBuf>) -> Result<Store> {
        let dir = dir.into();
        let buckets_dir = dir.join("buckets");
        fs::create_dir_all(&buckets_dir).map_err(io_error(&buckets_dir))?;

        let mut buckets = BTreeMap::new();
        for path in json_files(&buckets_dir)? {
            let bucket: Bucket = read_json(&path)?;
            buckets.insert(bucket.header.id, bucket);
        }

        let state_path = dir.join("state.json");
        let mut state: State = if state_path.exists() {
            read_json(&state_path)?
        } else {
            State::default()
        };
        if state.active.is_some_and(|id| !buckets.contains_key(&id)) {
            state.active = None;
        }

        let mut store = Store {
            dir,
            buckets,
            state,
        };
        if store.prune(DONE_RETENTION) > 0 {
            store.save()?;
        }
        Ok(store)
    }

    /// Writes every bucket and the state file, and deletes files of
    /// buckets that no longer exist.
    pub fn save(&self) -> Result<()> {
        let buckets_dir = self.dir.join("buckets");
        fs::create_dir_all(&buckets_dir).map_err(io_error(&buckets_dir))?;

        for bucket in self.buckets.values() {
            write_json(&bucket_path(&buckets_dir, bucket.header.id), bucket)?;
        }
        for path in json_files(&buckets_dir)? {
            let id = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .and_then(|stem| stem.parse::<Uuid>().ok());
            if id.is_some_and(|id| !self.buckets.contains_key(&id)) {
                fs::remove_file(&path).map_err(io_error(&path))?;
            }
        }
        write_json(&self.dir.join("state.json"), &self.state)
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    // --- Buckets ------------------------------------------------------------

    /// All buckets, in creation order.
    pub fn buckets(&self) -> impl Iterator<Item = &Bucket> {
        self.buckets.values()
    }

    /// Looks up a bucket by name (case-insensitive).
    pub fn bucket(&self, name: &str) -> Result<&Bucket> {
        Ok(&self.buckets[&self.bucket_id(name)?])
    }

    pub fn active_bucket(&self) -> Option<&Bucket> {
        self.state.active.and_then(|id| self.buckets.get(&id))
    }

    pub fn set_active_bucket(&mut self, name: &str) -> Result<()> {
        self.state.active = Some(self.bucket_id(name)?);
        Ok(())
    }

    /// Creates a bucket. The first bucket created becomes the active one.
    pub fn add_bucket(&mut self, name: &str) -> Result<&Bucket> {
        let name = valid_bucket_name(name)?;
        if self.bucket_id(name).is_ok() {
            return Err(Error::BucketExists(name.to_string()));
        }
        let bucket = Bucket::new(name);
        let id = bucket.header.id;
        self.buckets.insert(id, bucket);
        self.state.active.get_or_insert(id);
        Ok(&self.buckets[&id])
    }

    pub fn rename_bucket(&mut self, name: &str, new_name: &str) -> Result<()> {
        let id = self.bucket_id(name)?;
        let new_name = valid_bucket_name(new_name)?;
        if self.bucket_id(new_name).is_ok_and(|other| other != id) {
            return Err(Error::BucketExists(new_name.to_string()));
        }
        let bucket = self.buckets.get_mut(&id).expect("id comes from the map");
        bucket.name = new_name.to_string();
        bucket.header = bucket.header.update_edited();
        Ok(())
    }

    /// Removes a bucket and all its tasks. Fails if the bucket still has
    /// open tasks, unless `force` is set.
    pub fn remove_bucket(&mut self, name: &str, force: bool) -> Result<Bucket> {
        let id = self.bucket_id(name)?;
        let bucket = &self.buckets[&id];
        let open = bucket.open_count();
        if open > 0 && !force {
            return Err(Error::BucketNotEmpty {
                name: bucket.name.clone(),
                open,
            });
        }
        if self.state.active == Some(id) {
            self.state.active = None;
        }
        Ok(self.buckets.remove(&id).expect("id comes from the map"))
    }

    // --- Tasks --------------------------------------------------------------

    /// All tasks with their numbers. See [`Entry`] for the ordering.
    pub fn entries(&self) -> impl Iterator<Item = Entry<'_>> {
        self.buckets
            .values()
            .flat_map(|bucket| bucket.iter().map(move |task| (bucket, task)))
            .enumerate()
            .map(|(i, (bucket, task))| Entry {
                num: i + 1,
                bucket,
                task,
            })
    }

    /// Looks up a task by its number.
    pub fn entry(&self, num: usize) -> Result<Entry<'_>> {
        num.checked_sub(1)
            .and_then(|i| self.entries().nth(i))
            .ok_or(Error::TaskNumberNotFound(num))
    }

    /// Looks up a task by id.
    pub fn find(&self, id: Uuid) -> Result<Entry<'_>> {
        self.entries()
            .find(|entry| entry.task.header.id == id)
            .ok_or(Error::TaskNotFound(id))
    }

    /// Resolves a task number to the task's id.
    pub fn id_of(&self, num: usize) -> Result<Uuid> {
        Ok(self.entry(num)?.task.header.id)
    }

    /// Adds a task to the named bucket, or to the active bucket if `None`.
    /// Fails if the bucket does not exist.
    pub fn add_task(&mut self, bucket: Option<&str>, task: Task) -> Result<Uuid> {
        if task.description.trim().is_empty() {
            return Err(Error::EmptyDescription);
        }
        let bucket_id = match bucket {
            Some(name) => self.bucket_id(name)?,
            None => self.state.active.ok_or(Error::NoActiveBucket)?,
        };
        let bucket = self.buckets.get_mut(&bucket_id).expect("id is valid");
        Ok(bucket.add(task))
    }

    pub fn edit_task(&mut self, id: Uuid, edit: TaskEdit) -> Result<()> {
        if edit
            .description
            .as_deref()
            .is_some_and(|d| d.trim().is_empty())
        {
            return Err(Error::EmptyDescription);
        }
        self.task_mut(id)?.apply(edit);
        Ok(())
    }

    pub fn move_task(&mut self, id: Uuid, to_bucket: &str) -> Result<()> {
        let to = self.bucket_id(to_bucket)?;
        let from = self.bucket_of(id)?;
        if from != to {
            let task = self.buckets.get_mut(&from).unwrap().remove(id).unwrap();
            self.buckets.get_mut(&to).unwrap().add(task);
        }
        Ok(())
    }

    pub fn remove_task(&mut self, id: Uuid) -> Result<Task> {
        let from = self.bucket_of(id)?;
        Ok(self.buckets.get_mut(&from).unwrap().remove(id).unwrap())
    }

    /// Deletes done tasks completed more than `retention` ago.
    /// Returns the number of tasks deleted.
    pub fn prune(&mut self, retention: SignedDuration) -> usize {
        let cutoff = Timestamp::now()
            .checked_sub(retention)
            .unwrap_or(Timestamp::MIN);
        self.buckets
            .values_mut()
            .map(|bucket| {
                bucket.retain(|task| {
                    let completed = task.time_completed.unwrap_or(task.header.time_last_edited);
                    !(task.is_done() && completed < cutoff)
                })
            })
            .sum()
    }

    // --- Helpers ------------------------------------------------------------

    fn bucket_id(&self, name: &str) -> Result<Uuid> {
        let wanted = name.trim().to_lowercase();
        self.buckets
            .values()
            .find(|bucket| bucket.name.to_lowercase() == wanted)
            .map(|bucket| bucket.header.id)
            .ok_or_else(|| Error::BucketNotFound(name.trim().to_string()))
    }

    fn bucket_of(&self, task: Uuid) -> Result<Uuid> {
        self.buckets
            .values()
            .find(|bucket| bucket.get(task).is_some())
            .map(|bucket| bucket.header.id)
            .ok_or(Error::TaskNotFound(task))
    }

    fn task_mut(&mut self, id: Uuid) -> Result<&mut Task> {
        self.buckets
            .values_mut()
            .find_map(|bucket| bucket.get_mut(id))
            .ok_or(Error::TaskNotFound(id))
    }
}

fn valid_bucket_name(name: &str) -> Result<&str> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::EmptyBucketName);
    }
    Ok(name)
}

fn bucket_path(buckets_dir: &Path, id: Uuid) -> PathBuf {
    buckets_dir.join(format!("{id}.json"))
}

fn io_error(path: &Path) -> impl FnOnce(std::io::Error) -> Error + '_ {
    move |source| Error::Io {
        path: path.to_path_buf(),
        source,
    }
}

fn json_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in fs::read_dir(dir).map_err(io_error(dir))? {
        let path = entry.map_err(io_error(dir))?.path();
        if path.extension().is_some_and(|ext| ext == "json") {
            files.push(path);
        }
    }
    Ok(files)
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let text = fs::read_to_string(path).map_err(io_error(path))?;
    serde_json::from_str(&text).map_err(|source| Error::Json {
        path: path.to_path_buf(),
        source,
    })
}

/// Writes to a temporary file first and renames it, so a crash never
/// leaves a half-written file behind.
fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let text = serde_json::to_string_pretty(value).map_err(|source| Error::Json {
        path: path.to_path_buf(),
        source,
    })?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, text).map_err(io_error(&tmp))?;
    fs::rename(&tmp, path).map_err(io_error(path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::{Priority, Status};

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> TempDir {
            TempDir(std::env::temp_dir().join(format!("kru-test-{}", Uuid::new_v4())))
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn tasks_need_an_existing_bucket() {
        let dir = TempDir::new();
        let mut store = Store::open(&dir.0).unwrap();

        assert!(matches!(
            store.add_task(None, Task::new("a")),
            Err(Error::NoActiveBucket)
        ));
        assert!(matches!(
            store.add_task(Some("home"), Task::new("a")),
            Err(Error::BucketNotFound(_))
        ));

        store.add_bucket("Home").unwrap();
        store.add_task(Some("home"), Task::new("a")).unwrap();
        store.add_task(None, Task::new("b")).unwrap(); // first bucket is active
        assert_eq!(store.bucket("HOME").unwrap().len(), 2);
    }

    #[test]
    fn numbering_spans_buckets_and_survives_reload() {
        let dir = TempDir::new();
        let mut store = Store::open(&dir.0).unwrap();
        store.add_bucket("home").unwrap();
        store.add_bucket("work").unwrap();
        store.add_task(Some("home"), Task::new("h1")).unwrap();
        store.add_task(Some("work"), Task::new("w1")).unwrap();
        store.add_task(Some("home"), Task::new("h2")).unwrap();
        store.save().unwrap();

        let store = Store::open(&dir.0).unwrap();
        let listed: Vec<_> = store
            .entries()
            .map(|e| (e.num, e.bucket.name.as_str(), e.task.description.as_str()))
            .collect();
        assert_eq!(
            listed,
            [(1, "home", "h1"), (2, "home", "h2"), (3, "work", "w1")]
        );
        assert_eq!(store.active_bucket().unwrap().name, "home");
    }

    #[test]
    fn edit_move_and_remove() {
        let dir = TempDir::new();
        let mut store = Store::open(&dir.0).unwrap();
        store.add_bucket("home").unwrap();
        store.add_bucket("work").unwrap();
        let id = store.add_task(None, Task::new("a")).unwrap();

        store
            .edit_task(
                id,
                TaskEdit {
                    priority: Some(Priority::High),
                    status: Some(Status::Done),
                    ..TaskEdit::default()
                },
            )
            .unwrap();
        let task = store.find(id).unwrap().task;
        assert_eq!(task.priority, Priority::High);
        assert!(task.time_completed.is_some());

        store.move_task(id, "work").unwrap();
        assert_eq!(store.find(id).unwrap().bucket.name, "work");

        store.remove_task(id).unwrap();
        assert!(store.find(id).is_err());
    }

    #[test]
    fn removing_a_bucket_with_open_tasks_needs_force() {
        let dir = TempDir::new();
        let mut store = Store::open(&dir.0).unwrap();
        store.add_bucket("home").unwrap();
        store.add_task(None, Task::new("a")).unwrap();

        assert!(matches!(
            store.remove_bucket("home", false),
            Err(Error::BucketNotEmpty { open: 1, .. })
        ));
        store.remove_bucket("home", true).unwrap();
        assert!(store.active_bucket().is_none());

        store.save().unwrap();
        assert_eq!(json_files(&dir.0.join("buckets")).unwrap().len(), 0);
    }

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
}

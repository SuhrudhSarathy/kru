use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use jiff::{SignedDuration, Timestamp};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use uuid::Uuid;

use super::migrate::{self, File};
use super::{DONE_RETENTION, State, Store};
use crate::bucket::Bucket;
use crate::utils::error::{Error, Result};

impl Store {
    /// Opens the store in [`Store::default_dir`].
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
            let bucket: Bucket = read_versioned(&path, File::Bucket)?;
            buckets.insert(bucket.header.id, bucket);
        }

        // Check for the state file. If it doesn't exist, get a default state.
        let state_path = dir.join("state.json");
        let mut state: State = if state_path.exists() {
            read_versioned(&state_path, File::State)?
        } else {
            State::default()
        };

        // If the active bucket has somehow been deleted, reset it to none.
        // let the user set it
        if state.active.is_some_and(|id| !buckets.contains_key(&id)) {
            state.active = None;
        }

        // Create the store.
        let mut store = Store {
            dir,
            buckets,
            state,
        };

        // Prune the store based on retention.
        if store.prune(DONE_RETENTION) > 0 {
            store.save()?;
        }

        // All is good
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
    let json_error = |source| Error::Json {
        path: path.to_path_buf(),
        source,
    };
    let text = fs::read_to_string(path).map_err(io_error(path))?;
    let mut file: Value = serde_json::from_str(&text).map_err(json_error)?;

    let version = migrate::version(&file);
    if version > migrate::VERSION {
        return Err(Error::NewerFormat {
            path: path.to_path_buf(),
            version,
            supported: migrate::VERSION,
        });
    }
    let backup = path.with_extension(format!("json.v{version}.bak"));
    if version < migrate::VERSION && !backup.exists() {
        fs::copy(path, &backup).map_err(io_error(&backup))?;
    }
    migrate::upgrade(kind, &mut file, version);

    serde_json::from_value(file).map_err(json_error)
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
    use crate::task::{Status, Task};

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

        let store = Store::open(&dir.0).unwrap();
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

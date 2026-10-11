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
//!
//! The `impl Store` is split by responsibility:
//! - `store.rs`: the structs and simple getters
//! - `bucket.rs`: adding, renaming, removing and selecting buckets
//! - `task.rs`: numbering, looking up and changing tasks
//! - `io.rs`: opening, saving and pruning

mod bucket;
mod io;
mod migrate;
mod task;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use jiff::SignedDuration;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::bucket::Bucket;
use crate::task::Task;
use crate::utils::error::{Error, Result};
use directories::ProjectDirs;

/// How long done tasks are kept before [`Store::prune`] deletes them.
pub const DONE_RETENTION: SignedDuration = SignedDuration::from_hours(7 * 24);

/// Environment variable that overrides the store directory.
pub const DIR_ENV: &str = "KRU_DIR";

// This stores the UUID of the active bucket as of now
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
    // Directory where the json files are stored
    dir: PathBuf,

    // Datastructure to hold the Buckets
    buckets: BTreeMap<Uuid, Bucket>,

    // State: Active bucket uuid
    state: State,
}

impl Store {
    /// The directory the store reads from and writes to.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

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
}

#[cfg(test)]
mod test_util {
    use std::path::PathBuf;

    /// A unique directory under the system temp dir, deleted on drop.
    pub struct TempDir(pub PathBuf);

    impl TempDir {
        pub fn new() -> TempDir {
            TempDir(std::env::temp_dir().join(format!("kru-test-{}", uuid::Uuid::new_v4())))
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

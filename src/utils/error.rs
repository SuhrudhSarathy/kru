use std::io;
use std::path::PathBuf;

use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("bucket `{0}` does not exist")]
    BucketNotFound(String),

    #[error("bucket `{0}` already exists")]
    BucketExists(String),

    #[error("bucket `{name}` still has {open} open task(s)")]
    BucketNotEmpty { name: String, open: usize },

    #[error("no bucket given and no active bucket set")]
    NoActiveBucket,

    #[error("no task #{0}")]
    TaskNumberNotFound(usize),

    #[error("task {0} not found")]
    TaskNotFound(Uuid),

    #[error("no task matches `{0}` (expected a task number or id)")]
    NoTaskMatches(String),

    #[error("`{reference}` matches {count} tasks; use more of the id")]
    AmbiguousTask { reference: String, count: usize },

    #[error("{0} cannot be empty")]
    EmptyField(&'static str),

    #[error("{field} is too long ({len} characters, at most {max})")]
    TooLong {
        field: &'static str,
        len: usize,
        max: usize,
    },

    #[error("no field `{0}`")]
    UnknownField(String),

    #[error("{0} can't be edited")]
    ReadOnlyField(&'static str),

    #[error("invalid priority `{0}` (expected high, medium or low)")]
    InvalidPriority(String),

    #[error("invalid status `{0}` (expected todo, in-progress or done)")]
    InvalidStatus(String),

    #[error(
        "invalid due date `{0}` (try 2026-10-12, \"2026-10-12 17:00\", today, tomorrow, friday, 3d, 2h or none)"
    )]
    InvalidDue(String),

    #[error("could not determine a cache directory")]
    NoCacheDir,

    #[error("could not determine a config directory")]
    NoConfigDir,

    #[error("no config key `{0}` (expected dir or editor)")]
    UnknownConfigKey(String),

    #[error("{}: {source}", path.display())]
    Io { path: PathBuf, source: io::Error },

    #[error(
        "{}: written by a newer kru (file format {version}; this kru reads up to {supported}), upgrade kru",
        path.display()
    )]
    NewerFormat {
        path: PathBuf,
        version: u64,
        supported: u64,
    },

    #[error("{}: {source}", path.display())]
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },
}

pub type Result<T> = std::result::Result<T, Error>;

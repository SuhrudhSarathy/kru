//! Dynamic shell completion: the shell runs `kru` with `COMPLETE=<shell>`
//! set, and these functions suggest values from the store. Failures (no
//! store yet, a broken config) just mean no suggestions.

use std::ffi::OsStr;

use clap_complete::CompletionCandidate;
use kru::Store;
use kru::config::Config;
use kru::task::Status;

/// Bucket names, with their open task counts as help.
pub fn buckets(current: &OsStr) -> Vec<CompletionCandidate> {
    let Ok(store) = Store::open_default() else {
        return Vec::new();
    };
    let current = current.to_string_lossy();
    store
        .buckets()
        .filter(|b| b.name.starts_with(current.as_ref()))
        .map(|b| {
            CompletionCandidate::new(&b.name).help(Some(format!("{} open", b.open_count()).into()))
        })
        .collect()
}

/// Task numbers, with their descriptions as help.
pub fn tasks(current: &OsStr) -> Vec<CompletionCandidate> {
    task_candidates(current, |_| true)
}

/// Numbers of tasks that aren't done yet.
pub fn open_tasks(current: &OsStr) -> Vec<CompletionCandidate> {
    task_candidates(current, |status| status != Status::Done)
}

fn task_candidates(current: &OsStr, keep: impl Fn(Status) -> bool) -> Vec<CompletionCandidate> {
    let Ok(store) = Store::open_default() else {
        return Vec::new();
    };
    let current = current.to_string_lossy();
    store
        .entries()
        .filter(|e| keep(e.task.status))
        .map(|e| (e.num.to_string(), e))
        .filter(|(num, _)| num.starts_with(current.as_ref()))
        .map(|(num, e)| {
            let help = format!("{} [{}]", e.task.description, e.bucket.name);
            CompletionCandidate::new(num).help(Some(help.into()))
        })
        .collect()
}

/// Setting names for `kru config get/set/unset`.
pub fn config_keys(current: &OsStr) -> Vec<CompletionCandidate> {
    let current = current.to_string_lossy();
    Config::KEYS
        .iter()
        .filter(|key| key.starts_with(current.as_ref()))
        .map(CompletionCandidate::new)
        .collect()
}

//! Syncing a store across devices.
//!
//! Git moves the data and keeps its history; kru merges it. The store
//! directory is a git repository with an `origin` remote. Every task and
//! bucket has a stable uuid, so when two devices have both changed the
//! store, their versions are merged entity by entity with a three-way merge
//! against their common ancestor (see [`merge`]): changes made on one side
//! are taken, and when both sides changed the same field, the more recent
//! edit wins. Markdown notes are merged as text, and when both devices
//! changed the same lines, the more recently committed file wins.

mod git;
pub mod merge;
mod snapshot;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::process::Command;

use jiff::Timestamp;
use uuid::Uuid;

use crate::store::io::{lock, settle_active, write_snapshot};
use crate::utils::error::{Error, Result};
use git::Git;

pub use merge::{Conflict, Kind, Merged, What, merge};
pub use snapshot::Snapshot;

/// The branch kru syncs on.
const BRANCH: &str = "main";

/// The remote branch, as fetched.
const REMOTE_BRANCH: &str = "origin/main";

/// How many times to retry when another device pushed while syncing.
const ATTEMPTS: usize = 3;

/// Files that stay on this device: written into the store's `.gitignore`.
const DEVICE_LOCAL: &str = "\
# Device-local files, not synced (written by `kru sync init`)
state.json
.lock
.sync.lock
.last-sync
sync.log
*.bak
*.tmp
.DS_Store
";

/// What a sync did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// Commits brought in from the remote.
    pub pulled: usize,
    /// Commits sent to the remote.
    pub pushed: usize,
    /// Decisions made between conflicting changes.
    pub conflicts: Vec<Conflict>,
}

/// Where a store's sync stands, without contacting the remote.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    pub remote: String,
    pub last_sync: Option<Timestamp>,
    /// Changes not yet committed (they go out with the next sync).
    pub local_changes: bool,
    /// Commits not pushed yet / remote commits not merged yet, as of the
    /// last fetch.
    pub ahead: usize,
    pub behind: usize,
}

/// Whether the store in `dir` is set up for syncing.
pub fn is_set_up(dir: &Path) -> bool {
    dir.join(".git").exists()
}

/// Sets up syncing for the store in `dir` with the git `remote`, then
/// syncs. If the remote already holds a store (from another device), it is
/// merged with this one. `interactive` lets git ask for passwords.
pub fn init(dir: &Path, remote: &str, interactive: bool) -> Result<Report> {
    fs::create_dir_all(dir).map_err(|source| io(dir, source))?;
    let git = Git::new(dir, interactive)?;
    if !is_set_up(dir) {
        git.run(&["init", "-q"])?;
        git.run(&["symbolic-ref", "HEAD", &format!("refs/heads/{BRANCH}")])?;
    }
    let ignore = dir.join(".gitignore");
    if !ignore.exists() {
        fs::write(&ignore, DEVICE_LOCAL).map_err(|source| io(&ignore, source))?;
    }
    match git.run(&["remote", "get-url", "origin"]) {
        Ok(url) if url == remote => {}
        Ok(url) => return Err(Error::SyncRemoteMismatch(url)),
        Err(_) => {
            git.run(&["remote", "add", "origin", remote])?;
        }
    }
    run(dir, interactive)
}

/// Syncs the store in `dir`: commits local changes, brings in the remote's,
/// merges if both changed, and pushes. One sync runs at a time per store.
pub fn run(dir: &Path, interactive: bool) -> Result<Report> {
    if !is_set_up(dir) {
        return Err(Error::SyncNotSetUp);
    }
    let git = Git::new(dir, interactive)?;
    let _one_at_a_time = lock_file(dir, ".sync.lock")?;
    let mut report = Report::default();
    let mut redirects = BTreeMap::new();

    for attempt in 1..=ATTEMPTS {
        git.run(&["fetch", "-q", "origin"])?;
        let remote_exists = git.ok(&["rev-parse", "--verify", "-q", REMOTE_BRANCH]);

        // Commit and merge under the store lock, so no save lands halfway.
        {
            let _store = lock(dir)?;
            commit_local(&git)?;
            if remote_exists {
                let (ahead, behind) = ahead_behind(&git)?;
                if behind > 0 && ahead == 0 {
                    git.run(&["merge", "-q", "--ff-only", REMOTE_BRANCH])?;
                } else if behind > 0 {
                    redirects.extend(merge_diverged(&git, dir, &mut report)?);
                }
                report.pulled += behind;
            }
            settle_active(dir, &redirects)?;
        }

        let unpushed = if remote_exists {
            ahead_behind(&git)?.0
        } else {
            count(&git, "HEAD")?
        };
        if unpushed == 0 {
            break;
        }
        let pushed = git.output(&[
            "push",
            "-q",
            "-u",
            "origin",
            &format!("HEAD:refs/heads/{BRANCH}"),
        ])?;
        if pushed.status.success() {
            report.pushed += unpushed;
            break;
        }
        let message = git::failure_message(&pushed);
        let raced = ["rejected", "non-fast-forward", "fetch first"]
            .iter()
            .any(|sign| message.contains(sign));
        if !raced || attempt == ATTEMPTS {
            return Err(Error::Git {
                action: "push".into(),
                message,
            });
        }
        // Another device pushed meanwhile: fetch and merge again.
    }

    let stamp = dir.join(".last-sync");
    fs::write(&stamp, Timestamp::now().to_string()).map_err(|source| io(&stamp, source))?;
    Ok(report)
}

/// Where the store's sync stands, from what was last fetched.
pub fn status(dir: &Path) -> Result<Status> {
    if !is_set_up(dir) {
        return Err(Error::SyncNotSetUp);
    }
    let git = Git::new(dir, false)?;
    let (ahead, behind) = if git.ok(&["rev-parse", "--verify", "-q", REMOTE_BRANCH]) {
        ahead_behind(&git)?
    } else if git.ok(&["rev-parse", "--verify", "-q", "HEAD"]) {
        (count(&git, "HEAD")?, 0)
    } else {
        (0, 0)
    };
    Ok(Status {
        remote: git
            .run(&["remote", "get-url", "origin"])
            .unwrap_or_default(),
        last_sync: fs::read_to_string(dir.join(".last-sync"))
            .ok()
            .and_then(|text| text.trim().parse().ok()),
        local_changes: !git.run(&["status", "--porcelain"])?.is_empty(),
        ahead,
        behind,
    })
}

/// This device's name, for commit messages.
pub(crate) fn hostname() -> String {
    Command::new("hostname")
        .output()
        .ok()
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .filter(|name| !name.is_empty())
        .or_else(|| std::env::var("HOSTNAME").ok())
        .or_else(|| std::env::var("COMPUTERNAME").ok())
        .unwrap_or_else(|| "this device".into())
}

/// Commits everything changed in the store, if anything.
fn commit_local(git: &Git) -> Result<()> {
    git.run(&["add", "-A"])?;
    if !git.ok(&["diff", "--cached", "--quiet"]) {
        let message = format!("kru: changes on {}", hostname());
        git.run(&["commit", "-q", "-m", &message])?;
    }
    Ok(())
}

/// Merges the remote's changes when both sides have new commits: kru
/// merges the store itself, then records it as a git merge commit. Returns
/// which buckets were merged into which.
fn merge_diverged(git: &Git, dir: &Path, report: &mut Report) -> Result<BTreeMap<Uuid, Uuid>> {
    let base_rev = git.run(&["merge-base", "HEAD", REMOTE_BRANCH]).ok();
    let base = match &base_rev {
        Some(rev) => Snapshot::from_git(git, rev)?,
        None => Snapshot::default(),
    };
    let ours = Snapshot::from_git(git, "HEAD")?;
    let theirs = Snapshot::from_git(git, REMOTE_BRANCH)?;
    let merged = merge(&base, &ours, &theirs);

    // Start a merge that keeps our tree, then replace it with kru's result.
    let mut args = vec!["merge", "-q", "--no-commit", "--no-ff", "-s", "ours"];
    if base_rev.is_none() {
        args.push("--allow-unrelated-histories");
    }
    args.push(REMOTE_BRANCH);
    git.run(&args)?;

    write_snapshot(dir, &merged.snapshot)?;
    merge_notes(git, dir, base_rev.as_deref(), &merged.snapshot, report)?;

    git.run(&["add", "-A"])?;
    let message = format!("kru: merge on {}", hostname());
    git.run(&["commit", "-q", "-m", &message])?;
    report.conflicts.extend(merged.conflicts);
    Ok(merged.redirects)
}

/// Merges `notes/<task id>.md` files: unchanged on one side → the other
/// side's; changed on both → a text merge, and if the same lines changed,
/// the more recently committed file. Notes of deleted tasks are removed.
fn merge_notes(
    git: &Git,
    dir: &Path,
    base_rev: Option<&str>,
    merged: &Snapshot,
    report: &mut Report,
) -> Result<()> {
    let list = |rev: &str| -> Result<BTreeSet<String>> {
        let listing = git.run(&["ls-tree", "-r", "--name-only", rev, "--", "notes"])?;
        Ok(listing.lines().map(str::to_string).collect())
    };
    let ours_files = list("HEAD")?;
    let theirs_files = list(REMOTE_BRANCH)?;
    let base_files = match base_rev {
        Some(rev) => list(rev)?,
        None => BTreeSet::new(),
    };
    let read = |rev: &str, files: &BTreeSet<String>, path: &str| -> Result<Option<String>> {
        if files.contains(path) {
            git.file(rev, path).map(Some)
        } else {
            Ok(None)
        }
    };
    let live: BTreeSet<Uuid> = merged
        .buckets
        .values()
        .flat_map(|b| b.iter().map(|t| t.header.id))
        .collect();

    for path in ours_files.union(&theirs_files) {
        let task = Path::new(path)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .and_then(|stem| stem.parse::<Uuid>().ok());
        let file = dir.join(path);
        if task.is_some_and(|id| !live.contains(&id)) {
            let _ = fs::remove_file(&file);
            continue;
        }
        let ours = read("HEAD", &ours_files, path)?;
        let theirs = read(REMOTE_BRANCH, &theirs_files, path)?;
        let base = match base_rev {
            Some(rev) => read(rev, &base_files, path)?,
            None => None,
        };

        let result = if ours == theirs || theirs == base {
            ours
        } else if ours == base {
            theirs
        } else {
            match (&ours, &theirs) {
                (Some(o), Some(t)) => Some(match merge_text(git, o, base.as_deref(), t)? {
                    Some(text) => text,
                    None => {
                        let ours_newer = commit_time(git, "HEAD", path)?
                            >= commit_time(git, REMOTE_BRANCH, path)?;
                        report.conflicts.push(Conflict {
                            kind: Kind::Task,
                            id: task.unwrap_or_default(),
                            what: What::NotesOverwritten {
                                kept_this_device: ours_newer,
                            },
                        });
                        if ours_newer { o.clone() } else { t.clone() }
                    }
                }),
                // Deleted on one side, edited on the other: keep the edit.
                _ => ours.or(theirs),
            }
        };
        match result {
            Some(text) => {
                if let Some(folder) = file.parent() {
                    fs::create_dir_all(folder).map_err(|source| io(folder, source))?;
                }
                fs::write(&file, text).map_err(|source| io(&file, source))?;
            }
            None => {
                let _ = fs::remove_file(&file);
            }
        }
    }
    Ok(())
}

/// A three-way text merge with `git merge-file`. `None` if the same lines
/// changed on both sides.
fn merge_text(git: &Git, ours: &str, base: Option<&str>, theirs: &str) -> Result<Option<String>> {
    let scratch = std::env::temp_dir().join(format!("kru-merge-{}", Uuid::now_v7()));
    fs::create_dir_all(&scratch).map_err(|source| io(&scratch, source))?;
    let paths = [
        scratch.join("ours"),
        scratch.join("base"),
        scratch.join("theirs"),
    ];
    for (path, text) in paths.iter().zip([ours, base.unwrap_or(""), theirs]) {
        fs::write(path, text).map_err(|source| io(path, source))?;
    }
    let path_args: Vec<String> = paths.iter().map(|p| p.display().to_string()).collect();
    let output = git.output(&[
        "merge-file",
        "-p",
        &path_args[0],
        &path_args[1],
        &path_args[2],
    ]);
    let _ = fs::remove_dir_all(&scratch);
    let output = output?;
    Ok(match output.status.code() {
        Some(0) => Some(String::from_utf8_lossy(&output.stdout).into_owned()),
        _ => None,
    })
}

/// When the last commit touching `path` on `rev` was made.
fn commit_time(git: &Git, rev: &str, path: &str) -> Result<i64> {
    let seconds = git.run(&["log", "-1", "--format=%ct", rev, "--", path])?;
    Ok(seconds.parse().unwrap_or(0))
}

/// Commits on this side and on the remote that the other doesn't have.
fn ahead_behind(git: &Git) -> Result<(usize, usize)> {
    let counts = git.run(&[
        "rev-list",
        "--left-right",
        "--count",
        &format!("HEAD...{REMOTE_BRANCH}"),
    ])?;
    let mut parts = counts.split_whitespace().map(|n| n.parse().unwrap_or(0));
    Ok((parts.next().unwrap_or(0), parts.next().unwrap_or(0)))
}

/// How many commits `rev` has.
fn count(git: &Git, rev: &str) -> Result<usize> {
    Ok(git.run(&["rev-list", "--count", rev])?.parse().unwrap_or(0))
}

/// Holds an exclusive lock on `dir/name` until dropped.
fn lock_file(dir: &Path, name: &str) -> Result<fs::File> {
    let path = dir.join(name);
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .map_err(|source| io(&path, source))?;
    file.lock().map_err(|source| io(&path, source))?;
    Ok(file)
}

fn io(path: &Path, source: std::io::Error) -> Error {
    Error::Io {
        path: path.to_path_buf(),
        source,
    }
}

#[cfg(test)]
mod tests;

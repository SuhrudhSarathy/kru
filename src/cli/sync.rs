//! `kru sync`: keep tasks in step across devices through a git remote
//! (see [`kru::sync`]).

use std::fs::OpenOptions;
use std::io::{IsTerminal, Write};
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::Result;
use clap::Subcommand;
use jiff::Timestamp;
use kru::Store;
use kru::store::DIR_ENV;
use kru::sync::{self, Conflict, Kind, Report, What};
use serde_json::json;

use super::json;

#[derive(Subcommand)]
pub enum SyncCommand {
    /// Set up syncing with a git remote (e.g. an empty private GitHub
    /// repo), then sync. Run it on each device with the same remote.
    Init { remote: String },

    /// Show where syncing stands, without contacting the remote
    Status,
}

/// Runs `kru sync …`; no subcommand syncs now.
pub fn run(command: Option<SyncCommand>, store: &Store, as_json: bool) -> Result<()> {
    let dir = store.dir().to_path_buf();
    // Git may ask for a password only when someone can answer.
    let interactive = !as_json && std::io::stdin().is_terminal();
    let report = match command {
        None => sync::run(&dir, interactive)?,
        Some(SyncCommand::Init { remote }) => sync::init(&dir, &remote, interactive)?,
        Some(SyncCommand::Status) => return status(&dir, as_json),
    };
    print_report(&dir, &report, as_json)
}

fn print_report(dir: &Path, report: &Report, as_json: bool) -> Result<()> {
    let store = Store::open(dir)?;
    if as_json {
        let conflicts: Vec<_> = report
            .conflicts
            .iter()
            .map(|c| {
                json!({
                    "kind": match c.kind { Kind::Task => "task", Kind::Bucket => "bucket" },
                    "id": c.id,
                    "summary": c.summary(),
                })
            })
            .collect();
        json::print(json!({
            "pulled": report.pulled,
            "pushed": report.pushed,
            "conflicts": conflicts,
        }));
        return Ok(());
    }

    if report.pulled == 0 && report.pushed == 0 {
        println!("Already in sync.");
    } else {
        println!(
            "Synced: pulled {} change(s), pushed {}.",
            report.pulled, report.pushed
        );
    }
    if !report.conflicts.is_empty() {
        println!("Resolved {} conflict(s):", report.conflicts.len());
        for conflict in &report.conflicts {
            println!("  {}: {}", label(&store, conflict), conflict.summary());
        }
    }
    Ok(())
}

/// What a conflict was about, e.g. `#3 "Pay rent"` or `bucket home`.
fn label(store: &Store, conflict: &Conflict) -> String {
    match conflict.kind {
        Kind::Task => match store.find(conflict.id) {
            Ok(entry) => format!("#{} \"{}\"", entry.num, entry.task.description),
            Err(_) => format!("task {}", conflict.id),
        },
        Kind::Bucket => {
            let id = match conflict.what {
                What::BucketsMerged { into } => into,
                _ => conflict.id,
            };
            match store.buckets().find(|b| b.header.id == id) {
                Some(bucket) => format!("bucket {}", bucket.name),
                None => format!("bucket {id}"),
            }
        }
    }
}

fn status(dir: &Path, as_json: bool) -> Result<()> {
    let status = sync::status(dir)?;
    if as_json {
        json::print(json!({
            "remote": status.remote,
            "last_sync": status.last_sync.map(|at| at.to_string()),
            "local_changes": status.local_changes,
            "ahead": status.ahead,
            "behind": status.behind,
        }));
        return Ok(());
    }
    let last = match status.last_sync {
        Some(at) => format!(
            "{} ({})",
            kru::utils::due::format(at),
            kru::utils::due::relative(at, Timestamp::now())
        ),
        None => "never".into(),
    };
    println!("Remote:     {}", status.remote);
    println!("Last sync:  {last}");
    println!(
        "Here:       {}",
        if status.local_changes || status.ahead > 0 {
            "changes not synced yet"
        } else {
            "nothing new"
        }
    );
    if status.behind > 0 {
        println!(
            "Remote:     {} change(s) fetched but not merged yet",
            status.behind
        );
    }
    Ok(())
}

/// Starts `kru sync` in the background for the store in `dir` and returns
/// at once. Its output goes to `<dir>/sync.log`.
pub fn spawn_background(dir: &Path) -> Result<()> {
    let mut log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("sync.log"))?;
    writeln!(log, "--- {}", Timestamp::now())?;
    let mut command = Command::new(std::env::current_exe()?);
    command
        .arg("sync")
        .env(DIR_ENV, dir)
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log);
    detach(&mut command);
    command.spawn()?;
    Ok(())
}

/// Keeps the background sync running after the terminal it started from
/// closes.
#[cfg(unix)]
fn detach(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
}

#[cfg(windows)]
fn detach(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
}

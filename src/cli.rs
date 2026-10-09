//! Command-line interface. Parses arguments with clap and calls into
//! [`kru::Store`]; no task or bucket logic lives here.

mod bucket;
mod output;
mod task;

use anyhow::Result;
use clap::{Parser, Subcommand};
use jiff::SignedDuration;
use kru::Store;

#[derive(Parser)]
#[command(name = "kru", version, about = "Tasks, organised in buckets")]
#[command(after_help = "Run `kru` without a command to open the TUI.")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand)]
pub enum Command {
    /// Add, list, edit and remove tasks
    #[command(subcommand)]
    Task(task::TaskCommand),

    /// Add, list, rename and remove buckets
    #[command(subcommand)]
    Bucket(bucket::BucketCommand),

    /// Delete done tasks completed more than DAYS ago (runs automatically with 7 days)
    Prune {
        #[arg(long, default_value_t = 7)]
        days: u32,
    },

    /// Print where kru stores its data
    Path,
}

pub fn run(command: Command, store: &mut Store) -> Result<()> {
    match command {
        Command::Task(command) => task::run(command, store),
        Command::Bucket(command) => bucket::run(command, store),
        Command::Prune { days } => {
            let pruned = store.prune(SignedDuration::from_hours(i64::from(days) * 24));
            store.save()?;
            println!("Pruned {pruned} done task(s) older than {days} day(s).");
            Ok(())
        }
        Command::Path => {
            println!("{}", store.dir().display());
            Ok(())
        }
    }
}

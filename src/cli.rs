//! Command-line interface. Parses arguments with clap and calls into
//! [`kru::Store`]; no task or bucket logic lives here.

mod bucket;
pub mod config;
mod fields;
mod json;
mod output;
mod task;

use anyhow::Result;
use clap::{ArgMatches, CommandFactory, FromArgMatches, Parser, Subcommand};
use jiff::SignedDuration;
use kru::Store;

pub use fields::FieldInput;

#[derive(Parser)]
#[command(name = "kru", version, about = "Tasks, organised in buckets")]
#[command(after_help = "Run `kru` without a command to open the TUI.")]
pub struct Cli {
    /// Print JSON instead of text (for scripts)
    #[arg(long, global = true)]
    pub json: bool,

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

    /// Show or change settings: the store directory, the editor
    Config {
        #[command(subcommand)]
        command: Option<config::ConfigCommand>,
    },
}

/// Parses the command line: the command, plus any task field flags
/// (`--priority high`, …) given to `task add` or `task mark`.
pub fn parse() -> (Cli, Vec<FieldInput>) {
    let command = Cli::command().mut_subcommand("task", |task| {
        task.mut_subcommand("add", fields::add_flags)
            .mut_subcommand("mark", fields::add_flags)
    });
    let matches = command.get_matches();
    let cli = Cli::from_arg_matches(&matches).unwrap_or_else(|err| err.exit());
    let inputs = matches
        .subcommand_matches("task")
        .and_then(ArgMatches::subcommand)
        .map(|(_, matches)| fields::given(matches))
        .unwrap_or_default();
    (cli, inputs)
}

/// Runs a parsed command against the store. `fields` are the task field
/// flags from [`parse`]; `as_json` prints JSON instead of text.
pub fn run(
    command: Command,
    store: &mut Store,
    fields: &[FieldInput],
    as_json: bool,
) -> Result<()> {
    match command {
        Command::Task(command) => task::run(command, store, fields, as_json),
        Command::Bucket(command) => bucket::run(command, store, as_json),
        Command::Config { command } => config::run(command, as_json),
        Command::Prune { days } => {
            let pruned = store.prune(SignedDuration::from_hours(i64::from(days) * 24));
            store.save()?;
            if as_json {
                json::print(serde_json::json!({ "pruned": pruned }));
            } else {
                println!("Pruned {pruned} done task(s) older than {days} day(s).");
            }
            Ok(())
        }
        Command::Path => {
            if as_json {
                json::print(serde_json::json!({ "path": store.dir() }));
            } else {
                println!("{}", store.dir().display());
            }
            Ok(())
        }
    }
}

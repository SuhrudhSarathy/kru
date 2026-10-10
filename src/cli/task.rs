use anyhow::{Result, bail};
use clap::{Args, Subcommand};
use kru::Store;
use kru::task::{Priority, Status, Task};
use uuid::Uuid;

use super::{FieldInput, output};

#[derive(Subcommand)]
pub enum TaskCommand {
    /// Add a task to a bucket (the bucket must exist). Set its fields with
    /// the flags below; a description is required.
    Add {
        /// Bucket to add to [default: the active bucket]
        #[arg(short, long)]
        bucket: Option<String>,
    },

    /// List tasks; done tasks are hidden unless --all or --status done
    #[command(visible_alias = "ls")]
    List(ListArgs),

    /// Show every detail of a task
    Show { num: usize },

    /// Change a task's fields with the flags below
    #[command(visible_alias = "edit")]
    Mark {
        /// Task number, as shown by `kru task list`
        num: usize,
    },

    /// Mark one or more tasks as done
    Done {
        #[arg(required = true)]
        nums: Vec<usize>,
    },

    /// Move a task to another bucket
    #[command(visible_alias = "mv")]
    Move {
        num: usize,

        /// Destination bucket
        #[arg(short, long)]
        bucket: String,
    },

    /// Remove one or more tasks
    #[command(visible_alias = "rm")]
    Remove {
        #[arg(required = true)]
        nums: Vec<usize>,
    },
}

#[derive(Args)]
pub struct ListArgs {
    /// Only tasks in this bucket
    #[arg(short, long)]
    bucket: Option<String>,

    /// Only tasks with this status
    #[arg(short, long)]
    status: Option<Status>,

    /// Only tasks with this priority
    #[arg(short, long)]
    priority: Option<Priority>,

    /// Only tasks whose description contains this text
    #[arg(short = 'q', long)]
    query: Option<String>,

    /// Include done tasks
    #[arg(short, long)]
    all: bool,
}

/// Runs a `kru task …` command. `fields` are the field flags given to
/// `add` or `mark`.
pub fn run(command: TaskCommand, store: &mut Store, fields: &[FieldInput]) -> Result<()> {
    match command {
        TaskCommand::Add { bucket } => {
            let mut task = Task::new("");
            for (key, input) in fields {
                task.set(key, input)?;
            }
            let id = store.add_task(bucket.as_deref(), task)?;
            store.save()?;
            let entry = store.find(id)?;
            println!("Added task #{} to {}.", entry.num, entry.bucket.name);
        }

        TaskCommand::List(args) => list(store, args)?,

        TaskCommand::Show { num } => output::print_task_details(&store.entry(num)?),

        TaskCommand::Mark { num } => {
            if fields.is_empty() {
                bail!(
                    "nothing to change; pass any of {}",
                    super::fields::flag_list()
                );
            }
            let id = store.id_of(num)?;
            let changes = store.edit_task(id, fields)?;
            store.save()?;
            if changes.is_empty() {
                println!("#{num}: nothing changed.");
            }
            for change in changes {
                println!("#{num} {change}");
            }
        }

        TaskCommand::Done { nums } => {
            for (num, id) in resolve(store, nums)? {
                store.edit_task(id, &[("status", "done")])?;
                println!("Done #{num} {}", store.find(id)?.task.description);
            }
            store.save()?;
        }

        TaskCommand::Move { num, bucket } => {
            let id = store.id_of(num)?;
            store.move_task(id, &bucket)?;
            store.save()?;
            let entry = store.find(id)?;
            println!(
                "Moved task to {}; it is now #{}.",
                entry.bucket.name, entry.num
            );
        }

        TaskCommand::Remove { nums } => {
            // Resolve every number before removing anything, since removal
            // renumbers the tasks after it.
            for (num, id) in resolve(store, nums)? {
                let task = store.remove_task(id)?;
                println!("Removed #{num} {}", task.description);
            }
            store.save()?;
        }
    }
    Ok(())
}

/// Prints the tasks matching the filters, most urgent first (open tasks
/// before done ones). Done tasks are hidden unless `--all` or `--status`
/// is given.
fn list(store: &Store, args: ListArgs) -> Result<()> {
    let bucket = match &args.bucket {
        Some(name) => Some(store.bucket(name)?.header.id),
        None => None,
    };
    let query = args.query.map(|q| q.to_lowercase());
    let show_done = args.all || args.status.is_some();

    let mut hidden_done = 0;
    let mut entries: Vec<_> = store
        .entries()
        .filter(|e| bucket.is_none_or(|id| e.bucket.header.id == id))
        .filter(|e| args.status.is_none_or(|s| e.task.status == s))
        .filter(|e| args.priority.is_none_or(|p| e.task.priority == p))
        .filter(|e| {
            query
                .as_deref()
                .is_none_or(|q| e.task.description.to_lowercase().contains(q))
        })
        .filter(|e| {
            let hide = !show_done && e.task.is_done();
            hidden_done += usize::from(hide);
            !hide
        })
        .collect();
    entries.sort_by(|a, b| {
        a.task
            .is_done()
            .cmp(&b.task.is_done())
            .then_with(|| a.task.cmp_urgency(b.task))
    });

    if store.buckets().next().is_none() {
        println!("No buckets yet. Create one with `kru bucket add <name>`.");
    } else if entries.is_empty() {
        println!("No tasks.");
    } else {
        output::print_tasks(&entries);
    }
    if hidden_done > 0 {
        println!("({hidden_done} done task(s) hidden; use --all to show them)");
    }
    Ok(())
}

/// Turns task numbers into ids, deduplicated, failing on any unknown number.
fn resolve(store: &Store, mut nums: Vec<usize>) -> Result<Vec<(usize, Uuid)>> {
    nums.sort_unstable();
    nums.dedup();
    nums.into_iter()
        .map(|num| Ok((num, store.id_of(num)?)))
        .collect()
}

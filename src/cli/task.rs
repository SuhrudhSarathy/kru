use anyhow::{Result, bail};
use clap::{Args, Subcommand};
use kru::Store;
use kru::task::{Priority, Status, Task};
use uuid::Uuid;

use super::{FieldInput, json, output};

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
    Show {
        #[arg(help = TASK_HELP)]
        task: String,
    },

    /// Change a task's fields with the flags below
    #[command(visible_alias = "edit")]
    Mark {
        #[arg(help = TASK_HELP)]
        task: String,
    },

    /// Mark one or more tasks as done
    Done {
        #[arg(required = true, help = TASKS_HELP)]
        tasks: Vec<String>,
    },

    /// Move a task to another bucket
    #[command(visible_alias = "mv")]
    Move {
        #[arg(help = TASK_HELP)]
        task: String,

        /// Destination bucket
        #[arg(short, long)]
        bucket: String,
    },

    /// Remove one or more tasks
    #[command(visible_alias = "rm")]
    Remove {
        #[arg(required = true, help = TASKS_HELP)]
        tasks: Vec<String>,
    },
}

const TASK_HELP: &str = "Task number or id (or the start of an id), as shown by `kru task list`";
const TASKS_HELP: &str = "Task numbers or ids (or the start of ids)";

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
/// `add` or `mark`; `as_json` prints JSON instead of text.
pub fn run(
    command: TaskCommand,
    store: &mut Store,
    fields: &[FieldInput],
    as_json: bool,
) -> Result<()> {
    match command {
        TaskCommand::Add { bucket } => {
            let mut task = Task::new("");
            for (key, input) in fields {
                task.set(key, input)?;
            }
            let id = store.add_task(bucket.as_deref(), task)?;
            store.save()?;
            let entry = store.find(id)?;
            if as_json {
                json::print(json::entry(&entry));
            } else {
                println!("Added task #{} to {}.", entry.num, entry.bucket.name);
            }
        }

        TaskCommand::List(args) => list(store, args, as_json)?,

        TaskCommand::Show { task } => {
            let entry = store.find(store.resolve(&task)?)?;
            if as_json {
                json::print(json::entry(&entry));
            } else {
                output::print_task_details(&entry);
            }
        }

        TaskCommand::Mark { task } => {
            if fields.is_empty() {
                bail!(
                    "nothing to change; pass any of {}",
                    super::fields::flag_list()
                );
            }
            let id = store.resolve(&task)?;
            let changes = store.edit_task(id, fields)?;
            let num = store.find(id)?.num;
            store.save()?;
            if as_json {
                let changes: Vec<_> = changes.iter().map(json::change).collect();
                let task = json::entry(&store.find(id)?);
                json::print(serde_json::json!({ "task": task, "changes": changes }));
                return Ok(());
            }
            if changes.is_empty() {
                println!("#{num}: nothing changed.");
            }
            for change in changes {
                println!("#{num} {change}");
            }
        }

        TaskCommand::Done { tasks } => {
            let mut done = Vec::new();
            for (num, id) in resolve(store, &tasks)? {
                store.edit_task(id, &[("status", "done")])?;
                let entry = store.find(id)?;
                if as_json {
                    done.push(json::entry(&entry));
                } else {
                    println!("Done #{num} {}", entry.task.description);
                }
            }
            store.save()?;
            if as_json {
                json::print(done.into());
            }
        }

        TaskCommand::Move { task, bucket } => {
            let id = store.resolve(&task)?;
            store.move_task(id, &bucket)?;
            store.save()?;
            let entry = store.find(id)?;
            if as_json {
                json::print(json::entry(&entry));
            } else {
                println!(
                    "Moved task to {}; it is now #{}.",
                    entry.bucket.name, entry.num
                );
            }
        }

        TaskCommand::Remove { tasks } => {
            // Resolve every number before removing anything, since removal
            // renumbers the tasks after it.
            let mut removed = Vec::new();
            for (num, id) in resolve(store, &tasks)? {
                let bucket = store.find(id)?.bucket.name.clone();
                let task = store.remove_task(id)?;
                if as_json {
                    removed.push(json::task(num, &bucket, &task));
                } else {
                    println!("Removed #{num} {}", task.description);
                }
            }
            store.save()?;
            if as_json {
                json::print(removed.into());
            }
        }
    }
    Ok(())
}

/// Prints the tasks matching the filters, most urgent first (open tasks
/// before done ones). Done tasks are hidden unless `--all` or `--status`
/// is given.
fn list(store: &Store, args: ListArgs, as_json: bool) -> Result<()> {
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

    if as_json {
        json::print(json::entries(&entries));
        return Ok(());
    }
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

/// Resolves task references (numbers or ids) to `(number, id)` pairs,
/// deduplicated and in number order, failing on any that doesn't resolve.
/// Everything is resolved before anything changes, since changes can shift
/// task numbers.
fn resolve(store: &Store, references: &[String]) -> Result<Vec<(usize, Uuid)>> {
    let mut resolved = Vec::new();
    for reference in references {
        let id = store.resolve(reference)?;
        resolved.push((store.find(id)?.num, id));
    }
    resolved.sort_unstable();
    resolved.dedup();
    Ok(resolved)
}

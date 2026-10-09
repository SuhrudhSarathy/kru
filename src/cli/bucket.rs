use anyhow::{Result, bail};
use clap::Subcommand;
use kru::{Error, Store};

use super::output;

#[derive(Subcommand)]
pub enum BucketCommand {
    /// Create a bucket (the first one becomes the active bucket)
    Add { name: String },

    /// List buckets; `*` marks the active one
    #[command(visible_alias = "ls")]
    List,

    /// Make a bucket the default for `kru task add`
    Use { name: String },

    /// Rename a bucket
    Rename { name: String, new_name: String },

    /// Remove a bucket and all of its tasks
    #[command(visible_alias = "rm")]
    Remove {
        name: String,

        /// Remove it even if it still has open tasks
        #[arg(short, long)]
        force: bool,
    },
}

pub fn run(command: BucketCommand, store: &mut Store) -> Result<()> {
    match command {
        BucketCommand::Add { name } => {
            let bucket = store.add_bucket(&name)?;
            println!("Added bucket {}.", bucket.name);
            store.save()?;
        }

        BucketCommand::List => {
            if store.buckets().next().is_none() {
                println!("No buckets yet. Create one with `kru bucket add <name>`.");
            } else {
                output::print_buckets(store);
            }
        }

        BucketCommand::Use { name } => {
            store.set_active_bucket(&name)?;
            store.save()?;
            println!("Active bucket is now {}.", store.bucket(&name)?.name);
        }

        BucketCommand::Rename { name, new_name } => {
            store.rename_bucket(&name, &new_name)?;
            store.save()?;
            println!("Renamed {name} to {new_name}.");
        }

        BucketCommand::Remove { name, force } => {
            let bucket = match store.remove_bucket(&name, force) {
                Err(err @ Error::BucketNotEmpty { .. }) => {
                    bail!("{err}; use --force to remove it anyway")
                }
                result => result?,
            };
            store.save()?;
            println!(
                "Removed bucket {} and its {} task(s).",
                bucket.name,
                bucket.len()
            );
        }
    }
    Ok(())
}

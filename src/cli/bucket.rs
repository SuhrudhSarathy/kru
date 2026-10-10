use anyhow::{Result, bail};
use clap::Subcommand;
use kru::{Error, Store};

use super::{json, output};

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

/// Runs a `kru bucket …` command; `as_json` prints JSON instead of text.
pub fn run(command: BucketCommand, store: &mut Store, as_json: bool) -> Result<()> {
    match command {
        BucketCommand::Add { name } => {
            let name = store.add_bucket(&name)?.name.clone();
            store.save()?;
            if as_json {
                json::print(json::bucket(store, store.bucket(&name)?));
            } else {
                println!("Added bucket {name}.");
            }
        }

        BucketCommand::List => {
            if as_json {
                let buckets = store.buckets().map(|b| json::bucket(store, b)).collect();
                json::print(buckets);
            } else if store.buckets().next().is_none() {
                println!("No buckets yet. Create one with `kru bucket add <name>`.");
            } else {
                output::print_buckets(store);
            }
        }

        BucketCommand::Use { name } => {
            store.set_active_bucket(&name)?;
            store.save()?;
            let bucket = store.bucket(&name)?;
            if as_json {
                json::print(json::bucket(store, bucket));
            } else {
                println!("Active bucket is now {}.", bucket.name);
            }
        }

        BucketCommand::Rename { name, new_name } => {
            store.rename_bucket(&name, &new_name)?;
            store.save()?;
            let bucket = store.bucket(&new_name)?;
            if as_json {
                json::print(json::bucket(store, bucket));
            } else {
                println!("Renamed {name} to {}.", bucket.name);
            }
        }

        BucketCommand::Remove { name, force } => {
            let bucket = match store.remove_bucket(&name, force) {
                Err(err @ Error::BucketNotEmpty { .. }) => {
                    bail!("{err}; use --force to remove it anyway")
                }
                result => result?,
            };
            store.save()?;
            if as_json {
                json::print(json::bucket(store, &bucket));
            } else {
                println!(
                    "Removed bucket {} and its {} task(s).",
                    bucket.name,
                    bucket.len()
                );
            }
        }
    }
    Ok(())
}

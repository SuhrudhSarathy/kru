//! `kru config`: show and change settings (see [`kru::config::Config`]).
//! Runs without opening the store, so it works even if the configured
//! directory is broken.

use anyhow::Result;
use clap::Subcommand;
use clap_complete::ArgValueCompleter;
use kru::config::{Config, Source};
use serde_json::{Map, Value, json};

use super::complete;
use super::json as out;

#[derive(Subcommand)]
pub enum ConfigCommand {
    /// Show every setting, its value and where it comes from (the default)
    #[command(visible_alias = "ls")]
    List,

    /// Show one setting
    Get {
        #[arg(add = ArgValueCompleter::new(complete::config_keys))]
        key: String,
    },

    /// Change a setting (keys: dir, editor)
    Set {
        #[arg(add = ArgValueCompleter::new(complete::config_keys))]
        key: String,
        value: String,
    },

    /// Remove a setting, so it falls back to the environment or default
    Unset {
        #[arg(add = ArgValueCompleter::new(complete::config_keys))]
        key: String,
    },

    /// Print the config file's path
    Path,
}

/// Runs a `kru config …` command; no subcommand lists every setting.
pub fn run(command: Option<ConfigCommand>, as_json: bool) -> Result<()> {
    let mut config = Config::load()?;
    match command.unwrap_or(ConfigCommand::List) {
        ConfigCommand::List => {
            if as_json {
                let mut all = Map::new();
                for key in Config::KEYS {
                    all.insert(key.to_string(), setting(&config, key)?);
                }
                out::print(Value::Object(all));
            } else {
                let width = Config::KEYS.iter().map(|k| k.len()).max().unwrap_or(0);
                for key in Config::KEYS {
                    let (value, source) = config.effective(key)?;
                    println!("{key:<width$}  {value}  ({})", describe(source));
                }
                println!("\nConfig file: {}", Config::path()?.display());
            }
        }
        ConfigCommand::Get { key } => show(&config, &key, as_json)?,
        ConfigCommand::Set { key, value } => {
            config.set(&key, &value)?;
            config.save()?;
            show(&config, &key, as_json)?;
            if key == "dir" && !as_json {
                println!("Existing data is not moved; move it there yourself if you want it.");
            }
        }
        ConfigCommand::Unset { key } => {
            config.unset(&key)?;
            config.save()?;
            show(&config, &key, as_json)?;
        }
        ConfigCommand::Path => {
            let path = Config::path()?;
            if as_json {
                out::print(json!({ "path": path }));
            } else {
                println!("{}", path.display());
            }
        }
    }
    Ok(())
}

/// Prints one setting's effective value and its source.
fn show(config: &Config, key: &str, as_json: bool) -> Result<()> {
    if as_json {
        let mut value = setting(config, key)?;
        value["key"] = json!(key);
        out::print(value);
    } else {
        let (value, source) = config.effective(key)?;
        println!("{key} = {value}  ({})", describe(source));
    }
    Ok(())
}

/// `{"value": …, "source": …, "set": …}` for one key: what kru uses, where
/// it comes from (`config`, `env:NAME` or `default`), and what the config
/// file says (`null` if unset).
fn setting(config: &Config, key: &str) -> Result<Value> {
    let (value, source) = config.effective(key)?;
    let source = match source {
        Source::Config => "config".to_string(),
        Source::Env(var) => format!("env:{var}"),
        Source::Default => "default".to_string(),
    };
    Ok(json!({ "value": value, "source": source, "set": config.get(key)? }))
}

fn describe(source: Source) -> String {
    match source {
        Source::Config => "from config".to_string(),
        Source::Env(var) => format!("from ${var}"),
        Source::Default => "default".to_string(),
    }
}

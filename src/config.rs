//! User settings, kept in a small JSON file in the platform config folder
//! (e.g. `~/Library/Application Support/kru/config.json` on macOS,
//! `~/.config/kru/config.json` on Linux; `$KRU_CONFIG` overrides the path).
//!
//! It can't live in the store directory, since that directory is one of the
//! settings. Every key is optional; unset keys fall back to environment
//! variables and defaults (see [`Config::effective`]).

use std::fs;
use std::path::PathBuf;

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::store::{DIR_ENV, Store};
use crate::utils::error::{Error, Result};

/// Environment variable that overrides the config file's path.
pub const CONFIG_ENV: &str = "KRU_CONFIG";

/// The editor used when none is configured: one every system has.
const DEFAULT_EDITOR: &str = if cfg!(windows) { "notepad" } else { "vi" };

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    /// Where tasks are stored. `$KRU_DIR` takes precedence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dir: Option<PathBuf>,

    /// Command that opens a file for editing, e.g. `nvim` or `code --wait`.
    /// Takes precedence over `$VISUAL` and `$EDITOR`; the default is `vi`
    /// (`notepad` on Windows).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub editor: Option<String>,
}

/// Where a setting's effective value comes from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Source {
    Config,
    Env(&'static str),
    Default,
}

impl Config {
    /// Every setting, in display order.
    pub const KEYS: &[&str] = &["dir", "editor"];

    /// The config file's path: `$KRU_CONFIG`, or the platform config folder.
    pub fn path() -> Result<PathBuf> {
        if let Some(path) = std::env::var_os(CONFIG_ENV) {
            return Ok(path.into());
        }
        ProjectDirs::from("", "", "kru")
            .map(|dirs| dirs.config_dir().join("config.json"))
            .ok_or(Error::NoConfigDir)
    }

    /// Reads the config file; a missing file is an empty config.
    pub fn load() -> Result<Config> {
        let path = Config::path()?;
        if !path.exists() {
            return Ok(Config::default());
        }
        let text = fs::read_to_string(&path).map_err(|source| Error::Io {
            path: path.clone(),
            source,
        })?;
        serde_json::from_str(&text).map_err(|source| Error::Json { path, source })
    }

    /// Writes the config file, creating its folder if needed.
    pub fn save(&self) -> Result<()> {
        let path = Config::path()?;
        let io_error = |source| Error::Io {
            path: path.clone(),
            source,
        };
        if let Some(folder) = path.parent() {
            fs::create_dir_all(folder).map_err(io_error)?;
        }
        let text = serde_json::to_string_pretty(self).map_err(|source| Error::Json {
            path: path.clone(),
            source,
        })?;
        fs::write(&path, text).map_err(io_error)
    }

    /// The value set in the config file, if any.
    pub fn get(&self, key: &str) -> Result<Option<String>> {
        Ok(match key {
            "dir" => self.dir.as_ref().map(|dir| dir.display().to_string()),
            "editor" => self.editor.clone(),
            _ => return Err(unknown(key)),
        })
    }

    /// Sets a key in the config (call [`Config::save`] to keep it). A
    /// relative `dir` is made absolute.
    pub fn set(&mut self, key: &str, value: &str) -> Result<()> {
        let value = value.trim();
        if value.is_empty() {
            return Err(Error::EmptyField("config value"));
        }
        match key {
            "dir" => {
                let dir = std::path::absolute(value).map_err(|source| Error::Io {
                    path: value.into(),
                    source,
                })?;
                self.dir = Some(dir);
            }
            "editor" => self.editor = Some(value.to_string()),
            _ => return Err(unknown(key)),
        }
        Ok(())
    }

    /// Removes a key from the config, so it falls back to its default.
    pub fn unset(&mut self, key: &str) -> Result<()> {
        match key {
            "dir" => self.dir = None,
            "editor" => self.editor = None,
            _ => return Err(unknown(key)),
        }
        Ok(())
    }

    /// The value kru actually uses for `key`, and where it comes from.
    pub fn effective(&self, key: &str) -> Result<(String, Source)> {
        match key {
            "dir" => {
                if let Some(dir) = std::env::var_os(DIR_ENV) {
                    let dir = PathBuf::from(dir).display().to_string();
                    return Ok((dir, Source::Env(DIR_ENV)));
                }
                if let Some(dir) = &self.dir {
                    return Ok((dir.display().to_string(), Source::Config));
                }
                Ok((Store::cache_dir()?.display().to_string(), Source::Default))
            }
            "editor" => {
                if let Some(editor) = &self.editor {
                    return Ok((editor.clone(), Source::Config));
                }
                for var in ["VISUAL", "EDITOR"] {
                    if let Ok(editor) = std::env::var(var)
                        && !editor.trim().is_empty()
                    {
                        return Ok((editor, Source::Env(var)));
                    }
                }
                Ok((DEFAULT_EDITOR.to_string(), Source::Default))
            }
            _ => Err(unknown(key)),
        }
    }

    /// The editor command to run (see [`Config::effective`]).
    pub fn editor(&self) -> String {
        self.effective("editor")
            .map(|(editor, _)| editor)
            .unwrap_or_else(|_| DEFAULT_EDITOR.to_string())
    }
}

fn unknown(key: &str) -> Error {
    Error::UnknownConfigKey(key.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_can_be_set_read_and_unset() {
        let mut config = Config::default();
        config.set("editor", "  code --wait ").unwrap();
        config.set("dir", "relative/notes").unwrap();
        assert_eq!(
            config.get("editor").unwrap().as_deref(),
            Some("code --wait")
        );
        assert!(config.dir.as_ref().unwrap().is_absolute());

        // The config file's setting wins for the editor.
        assert_eq!(
            config.effective("editor").unwrap(),
            ("code --wait".to_string(), Source::Config)
        );

        config.unset("editor").unwrap();
        assert_eq!(config.get("editor").unwrap(), None);
        assert!(matches!(
            config.set("colour", "red"),
            Err(Error::UnknownConfigKey(_))
        ));
        assert!(matches!(
            config.set("editor", " "),
            Err(Error::EmptyField(_))
        ));
    }

    #[test]
    fn unset_keys_are_left_out_of_the_file() {
        let config = Config {
            editor: Some("nvim".into()),
            ..Config::default()
        };
        let text = serde_json::to_string(&config).unwrap();
        assert_eq!(text, r#"{"editor":"nvim"}"#);
        assert_eq!(serde_json::from_str::<Config>(&text).unwrap(), config);
        assert_eq!(
            serde_json::from_str::<Config>("{}").unwrap(),
            Config::default()
        );
    }
}

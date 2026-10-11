//! Running `git` in the store directory.
//!
//! kru shells out to the `git` program rather than linking a git library,
//! so the user's SSH keys, credential helpers and git config just work.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use crate::utils::error::{Error, Result};

pub(crate) struct Git {
    dir: PathBuf,
    /// Whether git may ask for passwords or passphrases on the terminal.
    interactive: bool,
    /// Commit identity to use when git has none configured.
    fallback_identity: Option<(String, String)>,
}

impl Git {
    /// A git runner for `dir`. Commands fail with [`Error::GitMissing`]
    /// if git isn't installed.
    pub(crate) fn new(dir: &Path, interactive: bool) -> Result<Git> {
        let mut git = Git {
            dir: dir.to_path_buf(),
            interactive,
            fallback_identity: None,
        };
        let configured = git.ok(&["config", "user.email"]);
        if !configured {
            let device = super::hostname();
            git.fallback_identity = Some((format!("kru on {device}"), format!("kru@{device}")));
        }
        Ok(git)
    }

    /// Runs git and returns its trimmed standard output.
    pub(crate) fn run(&self, args: &[&str]) -> Result<String> {
        let output = self.output(args)?;
        if !output.status.success() {
            return Err(Error::Git {
                action: args.first().copied().unwrap_or("").to_string(),
                message: failure_message(&output),
            });
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// The contents of `path` as committed at `rev`, exactly as stored.
    pub(crate) fn file(&self, rev: &str, path: &str) -> Result<String> {
        let spec = format!("{rev}:{path}");
        let output = self.output(&["show", &spec])?;
        if !output.status.success() {
            return Err(Error::Git {
                action: "show".into(),
                message: failure_message(&output),
            });
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    /// Runs git and reports whether it succeeded (for checks like "does
    /// this ref exist").
    pub(crate) fn ok(&self, args: &[&str]) -> bool {
        self.output(args)
            .is_ok_and(|output| output.status.success())
    }

    /// Runs git and returns its output whatever the exit status.
    pub(crate) fn output(&self, args: &[&str]) -> Result<Output> {
        let mut command = Command::new("git");
        command.arg("-C").arg(&self.dir).args(args);
        if !self.interactive {
            command.env("GIT_TERMINAL_PROMPT", "0");
            if std::env::var_os("GIT_SSH_COMMAND").is_none() {
                command.env("GIT_SSH_COMMAND", "ssh -o BatchMode=yes");
            }
        }
        if let Some((name, email)) = &self.fallback_identity {
            for (var, value) in [
                ("GIT_AUTHOR_NAME", name),
                ("GIT_COMMITTER_NAME", name),
                ("GIT_AUTHOR_EMAIL", email),
                ("GIT_COMMITTER_EMAIL", email),
            ] {
                command.env(var, value);
            }
        }
        command.output().map_err(|err| match err.kind() {
            std::io::ErrorKind::NotFound => Error::GitMissing,
            _ => Error::Git {
                action: args.first().copied().unwrap_or("").to_string(),
                message: err.to_string(),
            },
        })
    }
}

/// The most useful line of a failed git command's output.
pub(crate) fn failure_message(output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let lines: Vec<&str> = stderr
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("hint:"))
        .collect();
    if lines.is_empty() {
        format!("exited with {}", output.status)
    } else {
        lines.join("; ")
    }
}

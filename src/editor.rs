//! Opening files in the user's editor (see [`kru::config::Config::editor`]),
//! shared by the CLI and the TUI.

use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};

/// Runs `command` on `path` and waits for it to exit. The command may carry
/// arguments (`code --wait`); it is split on whitespace.
pub fn open(command: &str, path: &Path) -> Result<()> {
    let mut parts = command.split_whitespace();
    let program = parts.next().context("no editor configured")?;
    let status = Command::new(program)
        .args(parts)
        .arg(path)
        .status()
        .with_context(|| format!("couldn't start the editor `{command}`"))?;
    if !status.success() {
        bail!("the editor `{command}` exited with {status}");
    }
    Ok(())
}

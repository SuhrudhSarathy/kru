mod cli;
mod editor;
mod theme;
mod tui;

use anyhow::bail;
use kru::Store;

fn main() -> anyhow::Result<()> {
    let (cli, fields) = cli::parse();
    if cli.json && cli.command.is_none() {
        bail!("--json needs a command, e.g. `kru task list --json`");
    }
    match cli.command {
        // Settings don't need the store, which may be what's misconfigured.
        Some(cli::Command::Config { command }) => cli::config::run(command, cli.json),
        Some(command) => cli::run(command, &mut Store::open_default()?, &fields, cli.json),
        None => tui::run(Store::open_default()?),
    }
}

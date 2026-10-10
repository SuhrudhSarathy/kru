mod cli;
mod theme;
mod tui;

use anyhow::bail;
use kru::Store;

fn main() -> anyhow::Result<()> {
    let (cli, fields) = cli::parse();
    if cli.json && cli.command.is_none() {
        bail!("--json needs a command, e.g. `kru task list --json`");
    }
    let mut store = Store::open_default()?;
    match cli.command {
        Some(command) => cli::run(command, &mut store, &fields, cli.json),
        None => tui::run(store),
    }
}

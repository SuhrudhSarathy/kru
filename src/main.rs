mod cli;
mod editor;
mod theme;
mod tui;

use anyhow::bail;
use kru::Store;
use kru::config::Config;

fn main() -> anyhow::Result<()> {
    let (cli, fields) = cli::parse();
    if cli.json && cli.command.is_none() {
        bail!("--json needs a command, e.g. `kru task list --json`");
    }
    match cli.command {
        // Settings don't need the store, which may be what's misconfigured.
        Some(cli::Command::Config { command }) => cli::config::run(command, cli.json),
        Some(command) => {
            let mut store = Store::open_default()?;
            let changes_data = command.changes_data();
            cli::run(command, &mut store, &fields, cli.json)?;
            if changes_data && Config::load()?.autosync() && kru::sync::is_set_up(store.dir()) {
                cli::sync::spawn_background(store.dir())?;
            }
            Ok(())
        }
        None => tui::run(Store::open_default()?),
    }
}

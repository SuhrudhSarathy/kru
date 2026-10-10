mod cli;
mod theme;
mod tui;

use clap::Parser;
use kru::Store;

fn main() -> anyhow::Result<()> {
    let cli = cli::Cli::parse();
    let mut store = Store::open_default()?;
    match cli.command {
        Some(command) => cli::run(command, &mut store),
        None => tui::run(store),
    }
}

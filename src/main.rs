mod cli;
mod theme;
mod tui;

use kru::Store;

fn main() -> anyhow::Result<()> {
    let (cli, fields) = cli::parse();
    let mut store = Store::open_default()?;
    match cli.command {
        Some(command) => cli::run(command, &mut store, &fields),
        None => tui::run(store),
    }
}

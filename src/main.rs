mod app;
mod cli;
mod config;
mod lock;
mod registry;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Command};

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Init => app::init(),
        Command::Ps => app::ps(),
        Command::Deploy { app } => app::deploy(&app),
        Command::Rollback { app } => app::rollback(&app),
        Command::Stop { app } => app::stop(&app),
        Command::Logs { app } => app::logs(&app),
    }
}

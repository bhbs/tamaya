mod app;
mod cli;
mod config;
mod firecracker;
mod lock;
mod registry;
mod runtime;

use anyhow::Result;
use app::RunOptions;
use clap::Parser;
use cli::{Cli, Command};

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Init => app::init(),
        Command::Run {
            app,
            kernel,
            rootfs,
            firecracker_bin,
            tap,
            boot_args,
            vcpu,
            memory_mib,
            dry_run,
        } => app::run(RunOptions {
            app,
            kernel,
            rootfs,
            firecracker_bin,
            tap,
            boot_args,
            vcpu,
            memory_mib,
            dry_run,
        }),
        Command::Ps => app::ps(),
        Command::Deploy { app } => app::deploy(&app),
        Command::Rollback { app } => app::rollback(&app),
        Command::Stop { app } => app::stop(&app),
        Command::Logs { app } => app::logs(&app),
    }
}

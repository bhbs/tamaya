mod app;
mod builder;
mod cli;
mod config;
mod firecracker;
mod lock;
mod log;
mod registry;
mod runtime;
mod ssh;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Command};

fn main() -> Result<()> {
    log::init();
    let cli = Cli::parse();

    match cli.command {
        Command::Init => app::init(),
        Command::Build {
            app,
            context,
            dockerfile,
            artifact,
            dry_run,
        } => builder::build(builder::BuildOptions {
            app,
            context,
            dockerfile,
            artifact,
            dry_run,
        }),
        Command::Check {
            app,
            worker,
            kernel,
            rootfs,
            tap,
            skip_runtime,
            skip_capabilities,
            skip_kernel,
            skip_rootfs,
            skip_tap,
        } => app::check(app::CheckOptions {
            app,
            worker,
            kernel,
            rootfs,
            tap,
            skip_runtime,
            skip_capabilities,
            skip_kernel,
            skip_rootfs,
            skip_tap,
        }),
        Command::Ps => app::ps(),
        Command::Deploy {
            app,
            worker,
            kernel,
            rootfs,
            artifact,
            data,
            rootfs_size_mib,
            data_size_mib,
            firecracker_bin,
            tap,
            boot_args,
            vcpu,
            memory_mib,
            health_check_host,
            health_check_path,
            health_check_retries,
            health_check_interval_secs,
            health_check_timeout_secs,
            drain_seconds,
            skip_health_check,
            dry_run,
            domain,
        } => app::deploy(app::DeployOptions {
            app,
            worker,
            kernel,
            rootfs,
            artifact,
            data,
            rootfs_size_mib,
            data_size_mib,
            firecracker_bin,
            tap,
            boot_args,
            vcpu,
            memory_mib,
            health_check_host,
            health_check_path,
            health_check_retries,
            health_check_interval_secs,
            health_check_timeout_secs,
            drain_seconds,
            skip_health_check,
            dry_run,
            domain,
        }),
        Command::Rollback { app } => app::rollback(&app),
        Command::Stop { app } => app::stop(&app),
        Command::Logs { app } => app::logs(&app),
        Command::Setup { worker } => app::setup(app::SetupOptions { worker }),
        Command::Cleanup { worker, stale_taps } => {
            app::cleanup(app::CleanupOptions { worker, stale_taps })
        }
        Command::Unlock { app } => app::unlock(&app),
    }
}

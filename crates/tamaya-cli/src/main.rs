mod app;
mod cli;
mod config;
mod env;
mod log;
mod ssh;
mod validation;

use anyhow::{Context, Result};
use clap::Parser;
use cli::{Cli, Command, EnvAction};

fn main() -> Result<()> {
    log::init();
    let cli = Cli::parse();
    log::set_command(cli.command.name());
    log::log_start();
    let result = (|| {
        if let Some(dir) = cli.project_dir {
            std::env::set_current_dir(&dir)
                .with_context(|| format!("cannot change directory to {}", dir.display()))?;
        }
        match cli.command {
            Command::Setup { worker } => app::setup(worker),
            Command::Check { worker } => app::check(worker),
            Command::Deploy {
                app,
                worker,
                binary,
                domain,
                path,
                dry_run,
                verify_binary_deps,
            } => app::deploy(app::DeployOptions {
                app,
                worker,
                binary,
                domain,
                path,
                dry_run,
                verify_binary_deps,
            }),
            Command::Publish {
                app,
                worker,
                path,
                publish_type,
                static_root,
            } => app::publish(app::PublishOptions {
                app,
                worker,
                path,
                publish_type,
                static_root,
            }),
            Command::Rollback { app } => app::rollback(app),
            Command::Status { app } => app::status(app),
            Command::Stop { app } => app::stop(app),
            Command::Delete { app, purge } => app::delete(app, purge),
            Command::Logs { app } => app::logs(app),
            Command::Maintenance {
                app,
                domain,
                message,
            } => app::maintenance(app, domain, message),
            Command::Live { app, domain } => app::live(app, domain),
            Command::Version => app::version(),
            Command::Env { app, action } => match action {
                EnvAction::Set { key, stdin } => env::set(app.as_deref(), &key, stdin),
                EnvAction::Unset { key } => env::unset(app.as_deref(), &key),
                EnvAction::List => env::list(app.as_deref()),
            },
        }
    })();
    log::log_finish(result.is_ok());
    result
}

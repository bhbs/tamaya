mod deploy;
mod lifecycle;
mod publish;
mod status;
mod worker;

pub use deploy::{DeployOptions, deploy};
pub use lifecycle::{delete, live, logs, maintenance, rollback, stop};
pub use publish::{PublishOptions, publish};
pub use status::status;
pub use worker::{check, setup};

use crate::config::ProjectConfig;
use anyhow::{Context, Result};

pub fn version() -> Result<()> {
    println!("tamaya {}", env!("CARGO_PKG_VERSION"));
    Ok(())
}

fn context() -> Result<Option<ProjectConfig>> {
    crate::log::step("loading configuration");
    ProjectConfig::load()
}

pub(crate) fn app_name(arg: Option<&str>, project: Option<&ProjectConfig>) -> Result<String> {
    arg.map(str::to_owned)
        .or_else(|| project.and_then(|p| p.name.clone()))
        .context("app name is required; pass it as an argument or set name in .tamaya.toml")
}

fn runner(
    project: Option<&ProjectConfig>,
    selected: Option<&str>,
) -> Result<(String, crate::ssh::SshRunner)> {
    let (name, worker) = crate::config::worker_with_project(selected, project)?;
    Ok((name, crate::ssh::SshRunner::new(worker)))
}

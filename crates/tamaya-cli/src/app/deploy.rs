use super::{app_name, context, runner};
use crate::config::HealthCheckConfig;
use crate::ssh::DeployParams;
use crate::validation::{DomainIdentity, RouteKind, resolve_route, validate_app_name};
use anyhow::{Context, Result, bail};
use std::path::PathBuf;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DeployOptions {
    pub app: Option<String>,
    pub worker: Option<String>,
    pub binary: Option<PathBuf>,
    pub domain: Option<String>,
    pub path: Option<String>,
    pub dry_run: bool,
    pub verify_binary_deps: bool,
}

fn resolve_verify_binary_deps(
    options: &DeployOptions,
    project: Option<&crate::config::ProjectConfig>,
) -> bool {
    options.verify_binary_deps || project.is_some_and(|project| project.verify_binary_deps)
}

pub fn deploy(options: DeployOptions) -> Result<()> {
    let project = context()?;
    let verify_binary_deps = resolve_verify_binary_deps(&options, project.as_ref());
    let app = app_name(options.app.as_deref(), project.as_ref())?;
    validate_app_name(&app)?;
    if project.as_ref().is_some_and(|p| p.static_root.is_some()) {
        bail!("deploy does not support static_root; use tamaya publish for static sites");
    }
    let binary = options
        .binary
        .or_else(|| project.as_ref().and_then(|p| p.binary.clone()))
        .context("binary is required; pass --binary or set binary in .tamaya.toml")?;
    if !binary.is_file() {
        bail!("binary does not exist: {}", binary.display());
    }
    let domain = options
        .domain
        .or_else(|| project.as_ref().and_then(|p| p.domain.clone()));
    let domain = domain
        .map(DomainIdentity::parse)
        .transpose()?
        .map(|domain| domain.as_str().to_owned());
    let config_path = options
        .path
        .or_else(|| project.as_ref().and_then(|p| p.path.clone()));
    let route = resolve_route(domain.as_deref(), config_path.as_deref())?;
    let path_for_script = match route.kind {
        RouteKind::None => None,
        RouteKind::Root | RouteKind::Path => Some(route.path.as_str()),
    };
    let health = project
        .as_ref()
        .and_then(|p| p.health_check.clone())
        .unwrap_or_else(HealthCheckConfig::default);
    let memory = project
        .as_ref()
        .and_then(|p| p.memory.as_ref())
        .map(|m| m.max.as_str());
    let cpu = project
        .as_ref()
        .and_then(|p| p.cpu.as_ref())
        .map(|c| c.quota.as_str());
    let (worker_name, ssh) = runner(project.as_ref(), options.worker.as_deref())?;

    if options.dry_run {
        crate::log::step("resolved dry-run configuration");
        crate::log::result_ready();
        println!("deploy {app} to {worker_name}");
        println!("binary: {}", binary.display());
        println!("domain: {}", domain.as_deref().unwrap_or("(none)"));
        println!(
            "route: {} path: {}",
            route.kind.as_str(),
            path_for_script.unwrap_or("(none)")
        );
        println!(
            "health: {} retries={} interval={}s timeout={}s",
            health.path, health.retries, health.interval_secs, health.timeout_secs
        );
        println!("verify_binary_deps: {verify_binary_deps}");
        return Ok(());
    }

    crate::log::step("deploying binary");
    let output = ssh.deploy(
        &binary,
        &DeployParams {
            app: &app,
            domain: domain.as_deref(),
            path: path_for_script,
            route_kind: route.kind.as_str(),
            health: &health,
            memory_max: memory,
            cpu_quota: cpu,
            writable_release: project
                .as_ref()
                .is_some_and(|project| project.writable_release),
            verify_binary_deps,
        },
    )?;
    crate::log::result_ready();
    print!("{output}");
    Ok(())
}

use super::{app_name, context, runner};
use crate::validation::{
    DomainIdentity, PublishType, RouteKind, resolve_project_relative, resolve_route,
    validate_app_name,
};
use anyhow::{Context, Result, bail};
use std::path::PathBuf;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PublishOptions {
    pub app: Option<String>,
    pub worker: Option<String>,
    pub path: Option<String>,
    pub publish_type: Option<PublishType>,
    pub static_root: Option<PathBuf>,
}

pub fn publish(options: PublishOptions) -> Result<()> {
    let project = context()?;
    let project_path = crate::config::ProjectConfig::find_and_load()?.map(|(path, _)| path);
    let app = app_name(options.app.as_deref(), project.as_ref())?;
    validate_app_name(&app)?;

    if project.as_ref().is_some_and(|p| p.binary.is_some()) {
        bail!("publish does not support binary; use tamaya deploy for process apps");
    }

    let domain = project
        .as_ref()
        .and_then(|p| p.domain.clone())
        .context("domain is required for publish; set domain in .tamaya.toml")?;
    let domain = DomainIdentity::parse(domain)?;
    let config_path = options
        .path
        .or_else(|| project.as_ref().and_then(|p| p.path.clone()));
    let route = resolve_route(Some(domain.as_str()), config_path.as_deref())?;
    let path_for_script = match route.kind {
        RouteKind::Root | RouteKind::Path => route.path.as_str(),
        RouteKind::None => unreachable!("publish requires domain"),
    };
    let publish_type = options
        .publish_type
        .or_else(|| project.as_ref().and_then(|p| p.publish_type))
        .unwrap_or_default();
    let static_root = options
        .static_root
        .or_else(|| project.as_ref().and_then(|p| p.static_root.clone()))
        .context("static_root is required for publish; pass --static-root or set static_root in .tamaya.toml")?;
    let static_root = resolve_project_relative(static_root, project_path.as_deref());

    if !static_root.is_dir() {
        bail!(
            "static_root must be an existing directory: {}",
            static_root.display()
        );
    }
    let static_root = static_root
        .canonicalize()
        .with_context(|| format!("failed to resolve static_root {}", static_root.display()))?;
    if static_root.read_dir()?.next().is_none() {
        bail!("static_root must not be empty: {}", static_root.display());
    }
    match route.kind {
        RouteKind::Root => {
            if publish_type == PublishType::Spa && !static_root.join("index.html").is_file() {
                bail!("publish_type spa requires index.html in static_root");
            }
        }
        RouteKind::Path => {
            let relative = route.path.trim_start_matches('/');
            let prefix_dir = static_root.join(relative);
            if !prefix_dir.is_dir() {
                bail!(
                    "static_root must contain {} for path-based publish",
                    prefix_dir.display()
                );
            }
            if publish_type == PublishType::Spa && !prefix_dir.join("index.html").is_file() {
                bail!(
                    "publish_type spa requires index.html at {}",
                    prefix_dir.join("index.html").display()
                );
            }
        }
        RouteKind::None => {}
    }

    let (worker_name, ssh) = runner(project.as_ref(), options.worker.as_deref())?;
    crate::log::step(format!("publishing {app} to {worker_name}"));
    let output = ssh.publish(
        &static_root,
        &crate::ssh::PublishParams {
            app: &app,
            domain: domain.as_str(),
            path: path_for_script,
            route_kind: route.kind.as_str(),
            publish_type,
        },
    )?;
    crate::log::result_ready();
    print!("{output}");
    Ok(())
}

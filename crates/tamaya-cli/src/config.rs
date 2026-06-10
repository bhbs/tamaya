use anyhow::{Context, Result};
use serde::Deserialize;
use std::{env, fs, path::PathBuf};

use crate::validation::PublishType;

pub const PROJECT_CONFIG_FILE: &str = ".tamaya.toml";

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct WorkerConfig {
    pub alias: String,
    pub caddy_config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub port_start: u16,
    pub port_end: u16,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectConfig {
    #[serde(default)]
    pub worker: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub binary: Option<PathBuf>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub static_root: Option<PathBuf>,
    #[serde(default)]
    pub publish_type: Option<PublishType>,
    #[serde(default)]
    pub health_check: Option<HealthCheckConfig>,
    #[serde(default)]
    pub memory: Option<MemoryConfig>,
    #[serde(default)]
    pub cpu: Option<CpuConfig>,
    #[serde(default)]
    pub writable_release: bool,
    #[serde(default)]
    pub verify_binary_deps: bool,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HealthCheckConfig {
    #[serde(default = "default_health_path")]
    pub path: String,
    #[serde(default = "default_health_retries")]
    pub retries: u32,
    #[serde(default = "default_health_interval")]
    pub interval_secs: u32,
    #[serde(default = "default_health_timeout")]
    pub timeout_secs: u32,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryConfig {
    pub max: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CpuConfig {
    pub quota: String,
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            path: default_health_path(),
            retries: default_health_retries(),
            interval_secs: default_health_interval(),
            timeout_secs: default_health_timeout(),
        }
    }
}

fn default_caddy_config_dir() -> PathBuf {
    "/etc/caddy/conf.d".into()
}
fn default_data_dir() -> PathBuf {
    "/var/lib/tamaya".into()
}
fn default_port_start() -> u16 {
    20_000
}
fn default_port_end() -> u16 {
    29_999
}
fn default_health_path() -> String {
    "/health".into()
}
fn default_health_retries() -> u32 {
    5
}
fn default_health_interval() -> u32 {
    5
}
fn default_health_timeout() -> u32 {
    2
}

pub fn worker_with_project(
    selected: Option<&str>,
    project: Option<&ProjectConfig>,
) -> Result<(String, WorkerConfig)> {
    let name = selected
        .or_else(|| project.and_then(|p| p.worker.as_deref()))
        .context("worker is required; pass --worker or configure worker in .tamaya.toml")?;
    Ok((name.to_owned(), WorkerConfig::for_alias(name)))
}

impl ProjectConfig {
    pub fn find_and_load() -> Result<Option<(PathBuf, Self)>> {
        for dir in env::current_dir()?.ancestors() {
            let path = dir.join(PROJECT_CONFIG_FILE);
            if path.is_file() {
                let raw = fs::read_to_string(&path)?;
                return Ok(Some((
                    path.clone(),
                    toml::from_str(&raw)
                        .with_context(|| format!("failed to parse {}", path.display()))?,
                )));
            }
        }
        Ok(None)
    }
    pub fn load() -> Result<Option<Self>> {
        Ok(Self::find_and_load()?.map(|(_, c)| c))
    }
}

impl WorkerConfig {
    pub fn for_alias(alias: &str) -> Self {
        Self {
            alias: alias.to_owned(),
            caddy_config_dir: default_caddy_config_dir(),
            data_dir: default_data_dir(),
            port_start: default_port_start(),
            port_end: default_port_end(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_defaults_are_internal() {
        let worker = WorkerConfig::for_alias("prod");
        assert_eq!(worker.alias, "prod");
        assert_eq!(worker.data_dir, PathBuf::from("/var/lib/tamaya"));
        assert_eq!((worker.port_start, worker.port_end), (20_000, 29_999));
    }

    #[test]
    fn project_defaults_and_old_keys_are_rejected() {
        let project: ProjectConfig = toml::from_str("name = \"web\"").unwrap();
        assert_eq!(project.name.as_deref(), Some("web"));
        assert!(project.path.is_none());
        assert!(project.static_root.is_none());
        assert!(project.publish_type.is_none());
        assert!(toml::from_str::<ProjectConfig>("kernel = \"vmlinux\"").is_err());
        assert!(toml::from_str::<ProjectConfig>("default_worker = \"prod\"").is_err());
        assert!(toml::from_str::<ProjectConfig>("[workers.prod]").is_err());
    }

    #[test]
    fn project_accepts_process_and_publish_fields() {
        let process: ProjectConfig = toml::from_str(
            r#"name = "api"
domain = "example.com"
path = "/api"
binary = "./dist/api"
"#,
        )
        .unwrap();
        assert_eq!(process.path.as_deref(), Some("/api"));
        assert_eq!(process.binary, Some(PathBuf::from("./dist/api")));

        let published: ProjectConfig = toml::from_str(
            r#"name = "docs"
domain = "http://example.com"
path = "/docs"
publish_type = "spa"
static_root = "./dist"
"#,
        )
        .unwrap();
        assert_eq!(published.publish_type, Some(PublishType::Spa));
        assert_eq!(published.static_root, Some(PathBuf::from("./dist")));
    }

    #[test]
    fn health_defaults() {
        let health: HealthCheckConfig = toml::from_str("").unwrap();
        assert_eq!(health, HealthCheckConfig::default());
    }

    #[test]
    fn project_worker_selects_worker() {
        let project: ProjectConfig = toml::from_str(
            r#"worker = "staging"
"#,
        )
        .unwrap();
        assert_eq!(
            worker_with_project(None, Some(&project)).unwrap().1.alias,
            "staging"
        );
    }

    #[test]
    fn worker_selection_errors_without_project_or_flag() {
        assert!(worker_with_project(None, None).is_err());
        let (_, worker) = worker_with_project(Some("tamaya-prod"), None).unwrap();
        assert_eq!(worker.alias, "tamaya-prod");
    }
}

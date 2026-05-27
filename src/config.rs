use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::PathBuf;

pub const APP_ID: &str = "v";
pub const CONFIG_FILE: &str = "config.toml";

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct Config {
    pub images_dir: PathBuf,
    pub volumes_dir: PathBuf,
    pub runtime_dir: PathBuf,
    pub locks_dir: PathBuf,
    pub registry_file: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_worker: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub workers: BTreeMap<String, WorkerConfig>,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct WorkerConfig {
    pub host: String,
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default)]
    pub identity_file: Option<PathBuf>,
    pub firecracker_bin: String,
    #[serde(default = "default_caddy_config_dir")]
    pub caddy_config_dir: PathBuf,
}

fn default_caddy_config_dir() -> PathBuf {
    PathBuf::from("/etc/caddy/conf.d")
}

impl Config {
    pub fn default_from_env() -> Result<Self> {
        let dirs = XdgDirs::from_env()?;
        Ok(Self::default_for_xdg(&dirs))
    }

    pub fn default_for_xdg(dirs: &XdgDirs) -> Self {
        let data_dir = dirs.data_home.join(APP_ID);
        let state_dir = dirs.state_home.join(APP_ID);
        let runtime_dir = dirs
            .runtime_dir
            .clone()
            .unwrap_or_else(|| state_dir.join("runtime"));

        Self {
            images_dir: data_dir.join("images"),
            volumes_dir: data_dir.join("volumes"),
            runtime_dir: runtime_dir.join(APP_ID),
            locks_dir: runtime_dir.join(APP_ID).join("locks"),
            registry_file: state_dir.join("registry.toml"),
            default_worker: None,
            workers: BTreeMap::new(),
        }
    }

    #[cfg(test)]
    pub fn default_for(root: &std::path::Path) -> Self {
        let dirs = XdgDirs::for_home(root);
        Self {
            runtime_dir: root.join("runtime").join(APP_ID),
            locks_dir: root.join("runtime").join(APP_ID).join("locks"),
            ..Self::default_for_xdg(&dirs)
        }
    }

    pub fn load_from_env() -> Result<Self> {
        let path = config_path_from_env()?;
        let raw = fs::read_to_string(&path)
            .context(format!("failed to read config {}", path.display()))?;
        toml::from_str(&raw).context(format!("failed to parse config {}", path.display()))
    }

    pub fn save_to_env(&self) -> Result<()> {
        let path = config_path_from_env()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).context(format!(
                "failed to create config directory {}",
                parent.display()
            ))?;
        }
        let raw = toml::to_string_pretty(self).context("failed to serialize config")?;
        fs::write(&path, raw).context(format!("failed to write config {}", path.display()))
    }

    pub fn create_dirs(&self) -> Result<()> {
        for dir in [
            &self.images_dir,
            &self.volumes_dir,
            &self.runtime_dir,
            &self.locks_dir,
        ] {
            fs::create_dir_all(dir)
                .context(format!("failed to create directory {}", dir.display()))?;
        }

        Ok(())
    }

    pub fn worker(&self, selected: Option<&str>) -> Result<(&str, &WorkerConfig)> {
        let name = selected
            .or(self.default_worker.as_deref())
            .context("worker is required; pass --worker or set default_worker in config.toml")?;
        let (name, worker) = self
            .workers
            .get_key_value(name)
            .context(format!("worker {name:?} is not defined in config.toml"))?;

        Ok((name.as_str(), worker))
    }
}

impl WorkerConfig {
    pub fn ssh_target(&self) -> String {
        match &self.user {
            Some(user) => format!("{user}@{}", self.host),
            None => self.host.clone(),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct XdgDirs {
    pub config_home: PathBuf,
    pub data_home: PathBuf,
    pub state_home: PathBuf,
    pub runtime_dir: Option<PathBuf>,
}

impl XdgDirs {
    pub fn from_env() -> Result<Self> {
        let home = env::var_os("HOME")
            .map(PathBuf::from)
            .context("HOME is required when XDG_*_HOME is not set")?;

        Ok(Self {
            config_home: env_path("XDG_CONFIG_HOME").unwrap_or_else(|| home.join(".config")),
            data_home: env_path("XDG_DATA_HOME").unwrap_or_else(|| home.join(".local/share")),
            state_home: env_path("XDG_STATE_HOME").unwrap_or_else(|| home.join(".local/state")),
            runtime_dir: env_path("XDG_RUNTIME_DIR"),
        })
    }

    #[cfg(test)]
    pub fn for_home(home: &std::path::Path) -> Self {
        Self {
            config_home: home.join(".config"),
            data_home: home.join(".local/share"),
            state_home: home.join(".local/state"),
            runtime_dir: Some(home.join("runtime")),
        }
    }
}

pub fn config_path_from_env() -> Result<PathBuf> {
    Ok(XdgDirs::from_env()?
        .config_home
        .join(APP_ID)
        .join(CONFIG_FILE))
}

fn env_path(name: &str) -> Option<PathBuf> {
    let path = env::var_os(name).map(PathBuf::from)?;
    if path.is_absolute() { Some(path) } else { None }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn default_config_uses_xdg_directories() {
        let config = Config::default_for(Path::new("/tmp/project"));

        assert_eq!(
            config.images_dir,
            PathBuf::from("/tmp/project/.local/share/v/images")
        );
        assert_eq!(
            config.volumes_dir,
            PathBuf::from("/tmp/project/.local/share/v/volumes")
        );
        assert_eq!(config.runtime_dir, PathBuf::from("/tmp/project/runtime/v"));
        assert_eq!(
            config.locks_dir,
            PathBuf::from("/tmp/project/runtime/v/locks")
        );
        assert_eq!(
            config.registry_file,
            PathBuf::from("/tmp/project/.local/state/v/registry.toml")
        );
        assert_eq!(config.default_worker, None);
        assert!(config.workers.is_empty());
    }

    #[test]
    fn config_saves_and_loads() {
        let root = temp_project_dir("config-round-trip");
        let dirs = XdgDirs::for_home(&root);
        let config = Config::default_for_xdg(&dirs);
        config.create_dirs().expect("create config directories");
        let path = dirs.config_home.join(APP_ID).join(CONFIG_FILE);
        fs::create_dir_all(path.parent().expect("config parent")).expect("create config parent");
        fs::write(
            &path,
            toml::to_string_pretty(&config).expect("serialize config"),
        )
        .expect("write config");

        let raw = fs::read_to_string(path).expect("read config");
        let loaded: Config = toml::from_str(&raw).expect("parse config");

        assert_eq!(loaded, config);
        assert!(config.images_dir.is_dir());
        assert!(config.volumes_dir.is_dir());
        assert!(config.runtime_dir.is_dir());
        assert!(config.locks_dir.is_dir());

        fs::remove_dir_all(root).expect("remove temp project");
    }

    #[test]
    fn worker_config_uses_defaults_and_builds_ssh_target() {
        let raw = r#"
images_dir = "/project/.local/share/v/images"
volumes_dir = "/project/.local/share/v/volumes"
runtime_dir = "/project/runtime/v"
locks_dir = "/project/runtime/v/locks"
registry_file = "/project/.local/state/v/registry.toml"
default_worker = "prod"

[workers.prod]
host = "203.0.113.10"
user = "deploy"
firecracker_bin = "/usr/local/bin/firecracker"
"#;

        let config: Config = toml::from_str(raw).expect("parse worker config");
        let (name, worker) = config.worker(None).expect("resolve default worker");

        assert_eq!(name, "prod");
        assert_eq!(worker.ssh_target(), "deploy@203.0.113.10");
        assert_eq!(worker.port, None);
        assert_eq!(worker.firecracker_bin, "/usr/local/bin/firecracker");
        assert_eq!(worker.caddy_config_dir, PathBuf::from("/etc/caddy/conf.d"));
    }

    #[test]
    fn worker_selection_reports_missing_config() {
        let config = Config::default_for(Path::new("/tmp/project"));

        assert!(config.worker(None).is_err());
        assert!(config.worker(Some("prod")).is_err());
    }

    fn temp_project_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "v-{name}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time")
                .as_nanos()
        ))
    }
}

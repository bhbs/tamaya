use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const LOCAL_DIR: &str = ".v";
pub const CONFIG_FILE: &str = "config.toml";

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct Config {
    pub images_dir: PathBuf,
    pub volumes_dir: PathBuf,
    pub runtime_dir: PathBuf,
    pub locks_dir: PathBuf,
    pub registry_file: PathBuf,
}

impl Config {
    pub fn default_for(root: &Path) -> Self {
        let local_dir = root.join(LOCAL_DIR);

        Self {
            images_dir: local_dir.join("images"),
            volumes_dir: local_dir.join("volumes"),
            runtime_dir: local_dir.join("runtime"),
            locks_dir: local_dir.join("locks"),
            registry_file: local_dir.join("registry.toml"),
        }
    }

    pub fn load(root: &Path) -> Result<Self> {
        let path = config_path(root);
        let raw = fs::read_to_string(&path)
            .context(format!("failed to read config {}", path.display()))?;
        toml::from_str(&raw).context(format!("failed to parse config {}", path.display()))
    }

    pub fn save(&self, root: &Path) -> Result<()> {
        let path = config_path(root);
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
}

pub fn config_path(root: &Path) -> PathBuf {
    root.join(LOCAL_DIR).join(CONFIG_FILE)
}

pub fn ensure_local_dir(root: &Path) -> Result<()> {
    let path = root.join(LOCAL_DIR);
    fs::create_dir_all(&path).context(format!(
        "failed to create local directory {}",
        path.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn default_config_uses_local_directory() {
        let config = Config::default_for(Path::new("/tmp/project"));

        assert_eq!(config.images_dir, PathBuf::from("/tmp/project/.v/images"));
        assert_eq!(config.volumes_dir, PathBuf::from("/tmp/project/.v/volumes"));
        assert_eq!(config.runtime_dir, PathBuf::from("/tmp/project/.v/runtime"));
        assert_eq!(config.locks_dir, PathBuf::from("/tmp/project/.v/locks"));
        assert_eq!(
            config.registry_file,
            PathBuf::from("/tmp/project/.v/registry.toml")
        );
    }

    #[test]
    fn config_saves_and_loads() {
        let root = temp_project_dir("config-round-trip");
        ensure_local_dir(&root).expect("create local dir");

        let config = Config::default_for(&root);
        config.create_dirs().expect("create config directories");
        config.save(&root).expect("save config");

        let loaded = Config::load(&root).expect("load config");

        assert_eq!(loaded, config);
        assert!(config.images_dir.is_dir());
        assert!(config.volumes_dir.is_dir());
        assert!(config.runtime_dir.is_dir());
        assert!(config.locks_dir.is_dir());

        fs::remove_dir_all(root).expect("remove temp project");
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

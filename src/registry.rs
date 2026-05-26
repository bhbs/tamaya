use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct Registry {
    #[serde(default)]
    pub apps: BTreeMap<String, App>,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct App {
    pub current_image: Option<PathBuf>,
    pub previous_image: Option<PathBuf>,
    pub volume_path: PathBuf,
    pub port: u16,
    pub status: AppStatus,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AppStatus {
    Stopped,
    Starting,
    Running,
    Deploying,
    Failed,
}

impl Registry {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let raw = fs::read_to_string(path)
            .context(format!("failed to read registry {}", path.display()))?;
        toml::from_str(&raw).context(format!("failed to parse registry {}", path.display()))
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let parent = path
            .parent()
            .context(format!("registry path has no parent: {}", path.display()))?;
        fs::create_dir_all(parent).context(format!(
            "failed to create registry directory {}",
            parent.display()
        ))?;

        let raw = toml::to_string_pretty(self).context("failed to serialize registry")?;
        fs::write(path, raw).context(format!("failed to write registry {}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn empty_registry_round_trips() {
        let registry = Registry::default();
        let raw = toml::to_string(&registry).expect("serialize registry");
        let parsed: Registry = toml::from_str(&raw).expect("parse registry");

        assert_eq!(parsed, registry);
    }

    #[test]
    fn app_status_uses_kebab_case() {
        let mut registry = Registry::default();
        registry.apps.insert(
            "test".to_string(),
            App {
                current_image: None,
                previous_image: None,
                volume_path: PathBuf::from("/volumes/test"),
                port: 8080,
                status: AppStatus::Deploying,
            },
        );
        let status = toml::to_string(&registry).expect("serialize registry");

        assert!(status.contains("deploying"));
    }

    #[test]
    fn registry_loads_missing_file_as_empty() {
        let path = temp_registry_path("missing");

        let registry = Registry::load(&path).expect("load missing registry");

        assert!(registry.apps.is_empty());
    }

    #[test]
    fn registry_saves_and_loads_apps() {
        let path = temp_registry_path("round-trip");
        let mut registry = Registry::default();
        registry.apps.insert(
            "web".to_string(),
            App {
                current_image: Some(PathBuf::from("/images/web-v2.ext4")),
                previous_image: Some(PathBuf::from("/images/web-v1.ext4")),
                volume_path: PathBuf::from("/volumes/web"),
                port: 8080,
                status: AppStatus::Running,
            },
        );

        registry.save(&path).expect("save registry");
        let loaded = Registry::load(&path).expect("load registry");

        assert_eq!(loaded, registry);

        fs::remove_file(path).expect("remove registry");
    }

    #[test]
    fn registry_reports_parent_directory_create_errors() {
        let parent = temp_registry_path("parent-file");
        fs::write(&parent, "").expect("create parent file");
        let path = parent.join("registry.toml");

        let result = Registry::default().save(&path);

        assert!(result.is_err());

        fs::remove_file(parent).expect("remove parent file");
    }

    fn temp_registry_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "v-registry-{name}-{}.toml",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time")
                .as_nanos()
        ))
    }
}

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const API_SOCKET_FILE: &str = "firecracker.sock";
const LOG_DIR: &str = "logs";
const STATE_FILE: &str = "state.toml";

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RuntimeLayout {
    app: String,
    app_dir: PathBuf,
}

impl RuntimeLayout {
    pub fn from_runtime_dir(runtime_dir: &Path, app: impl Into<String>) -> Self {
        let app = app.into();
        let app_dir = runtime_dir.join(&app);

        Self { app, app_dir }
    }

    pub fn app(&self) -> &str {
        &self.app
    }

    pub fn app_dir(&self) -> &Path {
        &self.app_dir
    }

    pub fn api_socket_path(&self) -> PathBuf {
        self.app_dir.join(API_SOCKET_FILE)
    }

    pub fn log_dir(&self) -> PathBuf {
        self.app_dir.join(LOG_DIR)
    }

    pub fn state_file_path(&self) -> PathBuf {
        self.app_dir.join(STATE_FILE)
    }

    pub fn remove(&self) -> Result<()> {
        match fs::remove_dir_all(&self.app_dir) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error).context(format!(
                "failed to remove runtime directory {}",
                self.app_dir.display()
            )),
        }
    }

    pub fn create_dirs(&self) -> Result<()> {
        fs::create_dir_all(self.log_dir()).context(format!(
            "failed to create runtime directory {}",
            self.app_dir.display()
        ))
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeStatus {
    Starting,
    Running,
    Stopped,
    Unknown,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct RuntimeState {
    pub app: String,
    pub pid: Option<u32>,
    pub api_socket: PathBuf,
    pub status: RuntimeStatus,
    pub status_message: Option<String>,
}

impl RuntimeState {
    pub fn new(app: impl Into<String>, api_socket: PathBuf) -> Self {
        Self {
            app: app.into(),
            pid: None,
            api_socket,
            status: RuntimeStatus::Unknown,
            status_message: None,
        }
    }

    pub fn for_layout(layout: &RuntimeLayout) -> Self {
        Self::new(layout.app().to_owned(), layout.api_socket_path())
    }

    pub fn with_status(mut self, status: RuntimeStatus) -> Self {
        self.status = status;
        self
    }

    #[allow(dead_code)]
    pub fn with_pid(mut self, pid: u32) -> Self {
        self.pid = Some(pid);
        self
    }

    pub fn with_status_message(mut self, message: impl Into<String>) -> Self {
        self.status_message = Some(message.into());
        self
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let create_context = format!(
            "failed to create runtime state directory {}",
            parent.display()
        );
        fs::create_dir_all(parent).context(create_context)?;

        let raw = toml::to_string_pretty(self).context("failed to serialize runtime state")?;
        fs::write(path, raw).context(format!("failed to write runtime state {}", path.display()))
    }

    pub fn load(path: &Path) -> Result<Self> {
        let raw = fs::read_to_string(path)
            .context(format!("failed to read runtime state {}", path.display()))?;
        toml::from_str(&raw).context(format!("failed to parse runtime state {}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn layout_can_be_based_on_configured_runtime_dir() {
        let layout = RuntimeLayout::from_runtime_dir(Path::new("/tmp/project/.v/runtime"), "api");

        assert_eq!(layout.app_dir(), Path::new("/tmp/project/.v/runtime/api"));
        assert_eq!(layout.app(), "api");
        assert_eq!(
            layout.api_socket_path(),
            PathBuf::from("/tmp/project/.v/runtime/api/firecracker.sock")
        );
        assert_eq!(
            layout.log_dir(),
            PathBuf::from("/tmp/project/.v/runtime/api/logs")
        );
        assert_eq!(
            layout.state_file_path(),
            PathBuf::from("/tmp/project/.v/runtime/api/state.toml")
        );
    }

    #[test]
    fn create_dirs_creates_app_and_log_directories_only() {
        let root = temp_project_dir("runtime-layout");
        let layout = RuntimeLayout::from_runtime_dir(&root.join(".v/runtime"), "web");

        layout.create_dirs().expect("create runtime directories");

        assert!(layout.app_dir().is_dir());
        assert!(layout.log_dir().is_dir());
        assert!(!layout.api_socket_path().exists());
        assert!(!layout.state_file_path().exists());

        fs::remove_dir_all(root).expect("remove temp project");
    }

    #[test]
    fn remove_deletes_runtime_directory_and_allows_missing_directory() {
        let root = temp_project_dir("runtime-remove");
        let layout = RuntimeLayout::from_runtime_dir(&root.join(".v/runtime"), "web");

        layout.create_dirs().expect("create runtime directories");
        assert!(layout.app_dir().is_dir());

        layout.remove().expect("remove runtime directory");
        layout.remove().expect("remove missing runtime directory");
        assert!(!layout.app_dir().exists());
    }

    #[test]
    fn state_saves_and_loads_as_toml() {
        let root = temp_project_dir("runtime-state");
        let layout = RuntimeLayout::from_runtime_dir(&root.join(".v/runtime"), "web");
        let state = RuntimeState::for_layout(&layout)
            .with_status(RuntimeStatus::Running)
            .with_status_message("booted");

        state
            .save(&layout.state_file_path())
            .expect("save runtime state");
        let loaded = RuntimeState::load(&layout.state_file_path()).expect("load runtime state");

        assert_eq!(loaded, state);

        let raw = fs::read_to_string(layout.state_file_path()).expect("read runtime state");
        assert!(raw.contains("app = \"web\""));
        assert!(raw.contains("status = \"running\""));

        fs::remove_dir_all(root).expect("remove temp project");
    }

    #[test]
    fn state_reports_write_and_parse_errors() {
        let root = temp_project_dir("runtime-state-errors");
        let state = RuntimeState::new("web", PathBuf::from("/tmp/firecracker.sock"));

        assert!(state.save(Path::new("")).is_err());

        let invalid = root.join("invalid.toml");
        fs::create_dir_all(&root).expect("create temp project");
        fs::write(&invalid, "status = [").expect("write invalid runtime state");
        assert!(RuntimeState::load(&invalid).is_err());

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

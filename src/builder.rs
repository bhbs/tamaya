use crate::config::Config;
use crate::lock::{LockFile, app_lock_name, volume_lock_name};
use crate::registry::{App, AppStatus, Registry};
use crate::ssh::validate_remote_name;
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

const ARTIFACT_FILE: &str = "artifact.tar";
const DATA_SIZE_MIB: u64 = 256;
const APP_PORT: u16 = 3000;
const INIT_PATH: &str = "/sbin/init";
const DOCKER_PLATFORM: &str = "linux/amd64";

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct BuildOptions {
    pub app: String,
    pub context: PathBuf,
    pub dockerfile: PathBuf,
    pub artifact: Option<PathBuf>,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct BuildLayout {
    app_dir: PathBuf,
    artifact: PathBuf,
    config: PathBuf,
    metadata: PathBuf,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
struct AppConfig {
    app: String,
    artifact: PathBuf,
    port: u16,
    init: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
struct BuildMetadata {
    app: String,
    docker_context: PathBuf,
    dockerfile: PathBuf,
    image_tag: String,
    artifact: PathBuf,
    data_size_mib: u64,
    built_at_unix_secs: u64,
}

pub fn build(options: BuildOptions) -> Result<()> {
    validate_remote_name("app", &options.app)?;

    let config = Config::load_from_env()?;
    let layout = BuildLayout::new(&config, &options.app, options.artifact.clone())?;
    let image_tag = format!("v-{}:build", options.app);

    print_plan(&options, &layout, &image_tag);
    if options.dry_run {
        log::info!("build: dry-run; no Docker or filesystem commands were run");
        return Ok(());
    }

    let app_lock = LockFile::acquire(&config.locks_dir, &app_lock_name(&options.app))
        .with_context(|| {
            format!(
                "stale lock? try: rm {}/{}.lock",
                config.locks_dir.display(),
                app_lock_name(&options.app)
            )
        })?;
    let volume_lock = match LockFile::acquire(&config.locks_dir, &volume_lock_name(&options.app)) {
        Ok(lock) => lock,
        Err(e) => {
            drop(app_lock);
            return Err(e).with_context(|| {
                format!(
                    "stale lock? try: rm {}/{}.lock",
                    config.locks_dir.display(),
                    volume_lock_name(&options.app)
                )
            });
        }
    };
    let _app_lock = app_lock;
    let _volume_lock = volume_lock;

    if layout.app_dir.exists() {
        fs::remove_dir_all(&layout.app_dir)
            .context(format!("failed to remove {}", layout.app_dir.display()))?;
    }
    fs::create_dir_all(&layout.app_dir)
        .context(format!("failed to create {}", layout.app_dir.display()))?;
    if let Some(parent) = layout.artifact.parent() {
        fs::create_dir_all(parent).context(format!("failed to create {}", parent.display()))?;
    }

    let container = DockerContainer::create(&image_tag, &options)?;
    export_container_to_tar(container.id(), &layout.artifact)?;
    drop(container);

    write_json(
        &layout.config,
        &AppConfig {
            app: options.app.clone(),
            artifact: layout.artifact.clone(),
            port: APP_PORT,
            init: INIT_PATH.to_string(),
        },
    )?;
    write_json(
        &layout.metadata,
        &BuildMetadata {
            app: options.app.clone(),
            docker_context: options.context.clone(),
            dockerfile: options.dockerfile.clone(),
            image_tag,
            artifact: layout.artifact.clone(),
            data_size_mib: DATA_SIZE_MIB,
            built_at_unix_secs: now_unix_secs(),
        },
    )?;

    let mut registry = Registry::load(&config.registry_file)?;
    let previous = registry
        .apps
        .get(&options.app)
        .and_then(|app| app.current_image.clone());
    registry.apps.insert(
        options.app.clone(),
        App {
            current_image: Some(layout.artifact.clone()),
            previous_image: previous,
            volume_path: layout.app_dir.join("data.ext4"),
            port: APP_PORT,
            status: AppStatus::Stopped,
        },
    );
    registry.save(&config.registry_file)?;

    log::info!("build: wrote {}", layout.artifact.display());
    log::info!("build: wrote {}", layout.config.display());
    log::info!("build: wrote {}", layout.metadata.display());

    Ok(())
}

impl BuildLayout {
    fn new(config: &Config, app: &str, artifact: Option<PathBuf>) -> Result<Self> {
        let data_root = config
            .images_dir
            .parent()
            .context("images_dir has no parent")?;
        let app_dir = data_root.join("apps").join(app);
        Ok(Self {
            artifact: artifact.unwrap_or_else(|| app_dir.join(ARTIFACT_FILE)),
            config: app_dir.join("config.json"),
            metadata: app_dir.join("metadata.json"),
            app_dir,
        })
    }
}

struct DockerContainer {
    id: String,
}

impl DockerContainer {
    fn create(image_tag: &str, options: &BuildOptions) -> Result<Self> {
        run_status(
            Command::new("docker")
                .arg("build")
                .arg("--platform")
                .arg(DOCKER_PLATFORM)
                .arg("-t")
                .arg(image_tag)
                .arg("-f")
                .arg(&options.dockerfile)
                .arg(&options.context),
            "docker build",
        )?;

        let output = Command::new("docker")
            .arg("create")
            .arg(image_tag)
            .output()
            .context("failed to run docker create")?;
        if !output.status.success() {
            return bail_command("docker create", output.status, &output.stderr);
        }

        let id = String::from_utf8(output.stdout)
            .context("docker create output was not UTF-8")?
            .trim()
            .to_string();
        if id.is_empty() {
            bail!("docker create returned an empty container id");
        }

        Ok(Self { id })
    }

    fn id(&self) -> &str {
        &self.id
    }
}

impl Drop for DockerContainer {
    fn drop(&mut self) {
        let _ = Command::new("docker")
            .arg("rm")
            .arg("-f")
            .arg(&self.id)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

fn export_container_to_tar(container_id: &str, target: &PathBuf) -> Result<()> {
    let file =
        fs::File::create(target).context(format!("failed to create {}", target.display()))?;
    let status = Command::new("docker")
        .arg("export")
        .arg(container_id)
        .stdout(Stdio::from(file))
        .status()
        .context("failed to run docker export")?;
    if !status.success() {
        bail!("docker export failed: {status}");
    }
    Ok(())
}

fn run_status(command: &mut Command, label: &str) -> Result<()> {
    let output = command.output().context(format!("failed to run {label}"))?;
    if !output.status.success() {
        return bail_command(label, output.status, &output.stderr);
    }
    Ok(())
}

fn bail_command<T>(label: &str, status: std::process::ExitStatus, stderr: &[u8]) -> Result<T> {
    let stderr = String::from_utf8_lossy(stderr);
    bail!("{label} failed with {status}: {stderr}")
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let raw = serde_json::to_string_pretty(value).context("failed to serialize json")?;
    fs::write(path, format!("{raw}\n")).context(format!("failed to write {}", path.display()))
}

fn print_plan(options: &BuildOptions, layout: &BuildLayout, image_tag: &str) {
    log::info!("build: {}", options.app);
    log::info!("  app dir: {}", layout.app_dir.display());
    log::info!("  docker context: {}", options.context.display());
    log::info!("  dockerfile: {}", options.dockerfile.display());
    log::info!("  image tag: {image_tag}");
    log::info!("  platform: {DOCKER_PLATFORM}");
    log::info!("  artifact: {}", layout.artifact.display());
    log::info!("  config: {}", layout.config.display());
    log::info!("  metadata: {}", layout.metadata.display());
    log::info!("  port: {APP_PORT}");
    log::info!("  init: {INIT_PATH}");
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_layout_uses_app_scoped_files() {
        let config = Config::default_for(Path::new("/tmp/v-test"));
        let layout = BuildLayout::new(&config, "web", None).expect("build layout");

        assert_eq!(
            layout.app_dir,
            PathBuf::from("/tmp/v-test/.local/share/v/apps/web")
        );
        assert_eq!(
            layout.artifact,
            PathBuf::from("/tmp/v-test/.local/share/v/apps/web/artifact.tar")
        );
    }
}

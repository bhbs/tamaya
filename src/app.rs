use crate::config::Config;
use crate::firecracker::{
    BootPlan, BootSource, Drive, FirecrackerClient, MachineConfig, NetworkInterface,
};
use crate::lock::{LockFile, app_lock_name, volume_lock_name};
use crate::registry::Registry;
use crate::runtime::{RuntimeLayout, RuntimeState, RuntimeStatus};
use crate::ssh::{SshRunner, remote_runtime_dir_display};
use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RunOptions {
    pub app: String,
    pub worker: Option<String>,
    pub kernel: PathBuf,
    pub rootfs: PathBuf,
    pub firecracker_bin: PathBuf,
    pub tap: String,
    pub boot_args: String,
    pub vcpu: u8,
    pub memory_mib: u32,
    pub dry_run: bool,
}

pub fn init() -> Result<()> {
    let config = Config::default_from_env()?;
    config.create_dirs()?;
    config.save_to_env()?;

    let registry = Registry::load(&config.registry_file)?;
    registry.save(&config.registry_file)?;

    println!("initialized {}", config.registry_file.display());

    Ok(())
}

pub fn ps() -> Result<()> {
    let config = load_config()?;
    let registry = Registry::load(&config.registry_file)?;

    if registry.apps.is_empty() {
        println!("no apps");
        return Ok(());
    }

    for (name, app) in registry.apps {
        println!("{name}\t{:?}\t{}", app.status, app.port);
    }

    Ok(())
}

pub fn run(options: RunOptions) -> Result<()> {
    let config = load_config()?;
    let app = options.app.as_str();
    let _app_lock = LockFile::acquire(&config.locks_dir, &app_lock_name(app))?;

    let layout = RuntimeLayout::from_runtime_dir(&config.runtime_dir, app);
    layout.create_dirs()?;
    let worker = resolve_worker(&config, options.worker.as_deref(), options.dry_run)?;
    let mut remote_runtime = worker.map(|_| remote_runtime_dir_display(app));
    if !options.dry_run {
        let (_, worker) = worker.expect("worker is required for non dry-run");
        let runner = SshRunner::new(worker.clone());
        remote_runtime = Some(runner.create_runtime_dirs(app)?);
        runner.check_capabilities()?;
    }
    let api_socket_path = remote_runtime
        .as_deref()
        .map(|runtime| Path::new(runtime).join("firecracker.sock"))
        .unwrap_or_else(|| layout.api_socket_path());

    let plan = BootPlan {
        machine_config: MachineConfig::new(options.vcpu, options.memory_mib)?,
        boot_source: BootSource::new(options.kernel, options.boot_args)?,
        rootfs: Drive::rootfs(options.rootfs, true)?,
        network_interface: NetworkInterface::new("eth0", options.tap, None)?,
    };
    let client = FirecrackerClient::new(api_socket_path)?;
    let requests = client.build_boot_requests(&plan)?;
    let start_request = client.start_instance()?;
    let _request_bytes: usize = requests
        .iter()
        .chain(std::iter::once(&start_request))
        .map(|request| request.to_http_payload().len())
        .sum();

    let state = RuntimeState::for_layout(&layout)
        .with_status(RuntimeStatus::Starting)
        .with_status_message("boot plan prepared");

    if !options.dry_run {
        bail!(
            "remote worker execution is not implemented yet; run with --dry-run to inspect the boot plan"
        );
    }

    state.save(&layout.state_file_path())?;
    let _state = RuntimeState::load(&layout.state_file_path())?;

    println!("runtime: {}", layout.app_dir().display());
    if let Some((name, worker)) = worker {
        println!("worker: {name} ({})", worker.ssh_target());
        if let Some(remote_runtime) = &remote_runtime {
            println!("remote runtime: {remote_runtime}");
        }
    }
    println!("api socket: {}", client.api_socket_path().display());
    for request in requests {
        println!("{} {}", request.method, request.path);
    }
    println!("{} {}", start_request.method, start_request.path);
    if !options.dry_run {
        println!("pid: {}", state.pid.expect("running state has a pid"));
    }

    Ok(())
}

fn resolve_worker<'a>(
    config: &'a Config,
    selected: Option<&str>,
    dry_run: bool,
) -> Result<Option<(&'a str, &'a crate::config::WorkerConfig)>> {
    if dry_run && selected.is_none() && config.default_worker.is_none() {
        return Ok(None);
    }

    config.worker(selected).map(Some)
}

pub fn deploy(app: &str) -> Result<()> {
    let config = load_config()?;
    let _app_lock = LockFile::acquire(&config.locks_dir, &app_lock_name(app))?;
    let _volume_lock = LockFile::acquire(&config.locks_dir, &volume_lock_name(app))?;

    println!("deploy: deploying {app} is not implemented yet");

    Ok(())
}

pub fn rollback(app: &str) -> Result<()> {
    let config = load_config()?;
    let _app_lock = LockFile::acquire(&config.locks_dir, &app_lock_name(app))?;

    println!("rollback: rolling back {app} is not implemented yet");

    Ok(())
}

pub fn stop(app: &str) -> Result<()> {
    let config = load_config()?;
    let _app_lock = LockFile::acquire(&config.locks_dir, &app_lock_name(app))?;
    let layout = RuntimeLayout::from_runtime_dir(&config.runtime_dir, app);

    let state = match RuntimeState::load(&layout.state_file_path()) {
        Ok(state) => state,
        Err(error) if is_not_found_error(&error) => {
            println!("stop: {app} is not running");
            return Ok(());
        }
        Err(error) => return Err(error),
    };

    if let Some(pid) = state.pid {
        terminate_process(pid)?;
    }

    layout.remove()?;

    println!("stop: stopped {app}");

    Ok(())
}

pub fn logs(app: &str) -> Result<()> {
    let _config = load_config()?;

    println!("logs: showing logs for {app} is not implemented yet");

    Ok(())
}

fn load_config() -> Result<Config> {
    Config::load_from_env()
}

fn terminate_process(pid: u32) -> Result<()> {
    let status = Command::new("kill")
        .arg("-TERM")
        .arg(pid.to_string())
        .status()
        .context(format!("failed to terminate process {pid}"))?;

    if status.success() {
        return Ok(());
    }

    anyhow::bail!("failed to terminate process {pid}: {status}");
}

fn is_not_found_error(error: &anyhow::Error) -> bool {
    error
        .chain()
        .find_map(|cause| cause.downcast_ref::<std::io::Error>())
        .is_some_and(|io_error| io_error.kind() == std::io::ErrorKind::NotFound)
}

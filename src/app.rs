use crate::config::{Config, ensure_local_dir};
use crate::firecracker::{
    BootPlan, BootSource, Drive, FirecrackerClient, FirecrackerProcess, MachineConfig,
    NetworkInterface,
};
use crate::lock::{LockFile, app_lock_name, volume_lock_name};
use crate::registry::Registry;
use crate::runtime::{RuntimeLayout, RuntimeState, RuntimeStatus};
use anyhow::{Context, Result};
use std::env;
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RunOptions {
    pub app: String,
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
    let root = env::current_dir().context("failed to determine current directory")?;

    ensure_local_dir(&root)?;

    let config = Config::default_for(&root);
    config.create_dirs()?;
    config.save(&root)?;

    let registry = Registry::load(&config.registry_file)?;
    registry.save(&config.registry_file)?;

    println!(
        "initialized {}",
        root.join(crate::config::LOCAL_DIR).display()
    );

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

    let plan = BootPlan {
        machine_config: MachineConfig::new(options.vcpu, options.memory_mib)?,
        boot_source: BootSource::new(options.kernel, options.boot_args)?,
        rootfs: Drive::rootfs(options.rootfs, true)?,
        network_interface: NetworkInterface::new("eth0", options.tap, None)?,
    };
    let client = FirecrackerClient::new(layout.api_socket_path())?;
    let requests = client.build_boot_requests(&plan)?;
    let start_request = client.start_instance()?;
    let _request_bytes: usize = requests
        .iter()
        .chain(std::iter::once(&start_request))
        .map(|request| request.to_http_payload().len())
        .sum();

    let mut state = RuntimeState::for_layout(&layout)
        .with_status(RuntimeStatus::Starting)
        .with_status_message("boot plan prepared");

    if !options.dry_run {
        let process = FirecrackerProcess::start(
            &options.firecracker_bin,
            &layout.api_socket_path(),
            &layout.log_dir(),
        )?;
        state = state
            .with_pid(process.pid())
            .with_status_message("firecracker process started");
        state.save(&layout.state_file_path())?;

        if let Err(error) = client.boot(&plan) {
            let _ = terminate_process(process.pid());
            return Err(error).context("failed to boot microVM");
        }

        state = state
            .with_status(RuntimeStatus::Running)
            .with_status_message("microVM booted");
    }

    state.save(&layout.state_file_path())?;
    let _state = RuntimeState::load(&layout.state_file_path())?;

    println!("runtime: {}", layout.app_dir().display());
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
    let root = env::current_dir().context("failed to determine current directory")?;
    Config::load(&root)
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

use crate::config::{Config, ensure_local_dir};
use crate::firecracker::{
    BootPlan, BootSource, Drive, FirecrackerClient, MachineConfig, NetworkInterface,
};
use crate::lock::{LockFile, app_lock_name, volume_lock_name};
use crate::registry::Registry;
use crate::runtime::{RuntimeLayout, RuntimeState, RuntimeStatus};
use anyhow::{Context, Result};
use std::env;
use std::path::PathBuf;

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

pub fn run(
    app: &str,
    kernel: PathBuf,
    rootfs: PathBuf,
    tap: &str,
    boot_args: &str,
    vcpu: u8,
    memory_mib: u32,
) -> Result<()> {
    let config = load_config()?;
    let _app_lock = LockFile::acquire(&config.locks_dir, &app_lock_name(app))?;

    let layout = RuntimeLayout::from_runtime_dir(&config.runtime_dir, app);
    layout.create_dirs()?;

    let plan = BootPlan {
        machine_config: MachineConfig::new(vcpu, memory_mib)?,
        boot_source: BootSource::new(kernel, boot_args)?,
        rootfs: Drive::rootfs(rootfs, true)?,
        network_interface: NetworkInterface::new("eth0", tap, None)?,
    };
    let client = FirecrackerClient::new(layout.api_socket_path())?;
    let requests = client.build_boot_requests(&plan)?;
    let _request_bytes: usize = requests
        .iter()
        .map(|request| request.to_http_payload().len())
        .sum();

    RuntimeState::for_layout(&layout)
        .with_status(RuntimeStatus::Starting)
        .with_status_message("boot plan prepared")
        .save(&layout.state_file_path())?;
    let _state = RuntimeState::load(&layout.state_file_path())?;

    println!("runtime: {}", layout.app_dir().display());
    println!("api socket: {}", client.api_socket_path().display());
    for request in requests {
        println!("{} {}", request.method, request.path);
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

    println!("stop: stopping {app} is not implemented yet");

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

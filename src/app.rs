use crate::config::Config;
use crate::firecracker::{
    BootPlan, BootSource, Drive, FirecrackerClient, MachineConfig, NetworkInterface,
};
use crate::lock::{LockFile, app_lock_name, volume_lock_name};
use crate::registry::Registry;
use crate::runtime::{RuntimeLayout, RuntimeState, RuntimeStatus};
use crate::ssh::{SshRunner, remote_runtime_dir_display, validate_remote_name};
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

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CheckOptions {
    pub app: String,
    pub worker: Option<String>,
    pub kernel: Option<PathBuf>,
    pub rootfs: Option<PathBuf>,
    pub tap: String,
    pub skip_runtime: bool,
    pub skip_capabilities: bool,
    pub skip_kernel: bool,
    pub skip_rootfs: bool,
    pub skip_tap: bool,
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
    validate_remote_name("app", app)?;
    let _app_lock = LockFile::acquire(&config.locks_dir, &app_lock_name(app))?;

    let layout = RuntimeLayout::from_runtime_dir(&config.runtime_dir, app);
    layout.create_dirs()?;
    let worker = resolve_worker(&config, options.worker.as_deref(), options.dry_run)?;
    let mut remote_runtime = worker.map(|_| remote_runtime_dir_display(app));
    let mut kernel = options.kernel.clone();
    let mut rootfs = options.rootfs.clone();
    if !options.dry_run {
        let (_, worker) = worker.expect("worker is required for non dry-run");
        let runner = SshRunner::new(worker.clone());
        remote_runtime = Some(runner.create_runtime_dirs(app)?);
        runner.check_capabilities()?;
        kernel = prepare_boot_file(&runner, app, "kernel", &options.kernel)?;
        rootfs = prepare_boot_file(&runner, app, "rootfs", &options.rootfs)?;
        runner.ensure_tap_interface(&options.tap)?;
    }
    let api_socket_path = remote_runtime
        .as_deref()
        .map(|runtime| Path::new(runtime).join("firecracker.sock"))
        .unwrap_or_else(|| layout.api_socket_path());

    let plan = BootPlan {
        machine_config: MachineConfig::new(options.vcpu, options.memory_mib)?,
        boot_source: BootSource::new(kernel, options.boot_args)?,
        rootfs: Drive::rootfs(rootfs, true)?,
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
    let mut all_requests = requests.clone();
    all_requests.push(start_request.clone());

    let mut state = RuntimeState::new(app.to_string(), client.api_socket_path().to_path_buf())
        .with_status(RuntimeStatus::Starting)
        .with_status_message("boot plan prepared");
    if let Some((worker_name, _)) = worker {
        state = state.with_worker(worker_name.to_string());
    }
    if let Some(remote_runtime) = &remote_runtime {
        state = state.with_remote_runtime_dir(PathBuf::from(remote_runtime));
    }

    state.save(&layout.state_file_path())?;

    println!("runtime: {}", layout.app_dir().display());
    if let Some((name, worker)) = worker {
        println!("worker: {name} ({})", worker.ssh_target());
        if let Some(remote_runtime) = &remote_runtime {
            println!("remote runtime: {remote_runtime}");
        }
    }
    println!("api socket: {}", client.api_socket_path().display());
    println!("kernel: {}", plan.boot_source.kernel_image_path.display());
    println!("rootfs: {}", plan.rootfs.path_on_host.display());
    for request in requests {
        println!("{} {}", request.method, request.path);
    }
    println!("{} {}", start_request.method, start_request.path);
    if !options.dry_run {
        let (worker_name, worker) = worker.expect("worker is required for non dry-run");
        let remote_runtime = remote_runtime
            .as_deref()
            .context("remote runtime directory is required for non dry-run")?;
        let remote_log_dir = Path::new(remote_runtime).join("logs");
        let runner = SshRunner::new(worker.clone());
        let pid = runner.start_firecracker(
            &worker.firecracker_bin,
            client.api_socket_path(),
            &remote_log_dir,
        )?;
        RuntimeState::new(app.to_string(), client.api_socket_path().to_path_buf())
            .with_worker(worker_name.to_string())
            .with_remote_runtime_dir(PathBuf::from(remote_runtime))
            .with_pid(pid)
            .with_status(RuntimeStatus::Starting)
            .with_status_message("Firecracker started")
            .save(&layout.state_file_path())?;
        runner.send_firecracker_api_requests(client.api_socket_path(), &all_requests)?;

        RuntimeState::new(app.to_string(), client.api_socket_path().to_path_buf())
            .with_worker(worker_name.to_string())
            .with_remote_runtime_dir(PathBuf::from(remote_runtime))
            .with_pid(pid)
            .with_status(RuntimeStatus::Running)
            .with_status_message("booted")
            .save(&layout.state_file_path())?;
        println!("pid: {pid}");
    }

    Ok(())
}

pub fn check(options: CheckOptions) -> Result<()> {
    let config = load_config()?;
    let app = options.app.as_str();
    validate_remote_name("app", app)?;
    let (worker_name, worker) = config.worker(options.worker.as_deref())?;
    let runner = SshRunner::new(worker.clone());

    let remote_runtime = if options.skip_runtime {
        None
    } else {
        Some(runner.create_runtime_dirs(app)?)
    };
    if !options.skip_capabilities {
        runner.check_capabilities()?;
    }
    let kernel = if options.skip_kernel {
        None
    } else {
        let kernel = options
            .kernel
            .as_deref()
            .context("kernel path is required unless --skip-kernel is passed")?;
        Some(runner.require_readable_file("kernel", kernel)?)
    };
    let rootfs = if options.skip_rootfs {
        None
    } else {
        let rootfs = options
            .rootfs
            .as_deref()
            .context("rootfs path is required unless --skip-rootfs is passed")?;
        Some(runner.require_readable_file("rootfs", rootfs)?)
    };
    let tap = if options.skip_tap {
        None
    } else {
        Some(runner.require_tap_interface(&options.tap)?)
    };

    println!("worker: {worker_name} ({})", worker.ssh_target());
    if let Some(remote_runtime) = &remote_runtime {
        println!("remote runtime: {remote_runtime}");
        println!(
            "api socket: {}",
            Path::new(remote_runtime).join("firecracker.sock").display()
        );
    }
    if let Some(kernel) = kernel {
        println!("kernel: {kernel}");
    }
    if let Some(rootfs) = rootfs {
        println!("rootfs: {rootfs}");
    }
    if let Some(tap) = tap {
        println!("tap: {tap}");
    }
    println!("ok");

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

fn prepare_boot_file(runner: &SshRunner, app: &str, kind: &str, path: &Path) -> Result<PathBuf> {
    if path.is_file() {
        return runner
            .upload_boot_file(app, kind, path)
            .map(PathBuf::from)
            .context(format!("failed to upload local {kind} {}", path.display()));
    }

    if !is_remote_boot_path(path) {
        bail!(
            "local {kind} file does not exist: {}; pass an existing local file or a worker-side absolute/XDG path",
            path.display()
        );
    }

    runner
        .require_readable_file(kind, path)
        .map(PathBuf::from)
        .context(format!(
            "failed to validate remote {kind} {}",
            path.display()
        ))
}

fn is_remote_boot_path(path: &Path) -> bool {
    path.is_absolute()
        || path.to_str().is_some_and(|value| {
            value.starts_with("$XDG_DATA_HOME/")
                || value.starts_with("$XDG_STATE_HOME/")
                || value.starts_with("$XDG_RUNTIME_DIR/")
        })
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

    let stopped_pid = state.pid;
    if let Some(worker_name) = &state.worker {
        if let (Some(pid), Some(remote_runtime_dir)) =
            (state.pid, state.remote_runtime_dir.as_ref())
        {
            let (_, worker) = config.worker(Some(worker_name))?;
            let runner = SshRunner::new(worker.clone());
            runner.stop_firecracker(pid, remote_runtime_dir)?;
        }
    } else if let Some(pid) = state.pid {
        terminate_process(pid)?;
    }

    layout.remove()?;

    match stopped_pid {
        Some(pid) => println!("stop: stopped {app} pid {pid}"),
        None => println!("stop: stopped {app}"),
    }

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

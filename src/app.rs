use crate::config::Config;
use crate::config::WorkerConfig;
use crate::firecracker::{
    BootPlan, BootSource, Drive, FirecrackerClient, MachineConfig, NetworkInterface,
};
use crate::lock::{LockFile, app_lock_name, volume_lock_name};
use crate::progress;
use crate::registry::{AppStatus, Registry};
use crate::runtime::{RuntimeLayout, RuntimeState, RuntimeStatus};
use crate::ssh::{SshRunner, validate_remote_name};
use anyhow::{Context, Result, bail};
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CheckOptions {
    pub app: String,
    pub worker: Option<String>,
    pub kernel: Option<PathBuf>,
    pub rootfs: Option<PathBuf>,
    pub tap: Option<String>,
    pub skip_runtime: bool,
    pub skip_capabilities: bool,
    pub skip_kernel: bool,
    pub skip_rootfs: bool,
    pub skip_tap: bool,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SetupOptions {
    pub worker: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CleanupOptions {
    pub worker: Option<String>,
    pub stale_taps: bool,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DeployOptions {
    pub app: String,
    pub worker: Option<String>,
    pub kernel: PathBuf,
    pub rootfs: Option<PathBuf>,
    pub artifact: Option<PathBuf>,
    pub data: Option<PathBuf>,
    pub rootfs_size_mib: u64,
    pub data_size_mib: u64,
    pub firecracker_bin: PathBuf,
    pub tap: String,
    pub boot_args: String,
    pub vcpu: u8,
    pub memory_mib: u32,
    pub health_check_host: String,
    pub health_check_path: Option<String>,
    pub health_check_retries: u32,
    pub health_check_interval_secs: u32,
    pub health_check_timeout_secs: u32,
    pub drain_seconds: u32,
    pub skip_health_check: bool,
    pub dry_run: bool,
    pub domain: Option<String>,
}

#[derive(Clone)]
struct VmBootContext {
    pid: u32,
    remote_runtime_dir: PathBuf,
}

/// Guard that cleans up a failed deploy attempt on drop.
/// Call `disarm()` once the new VM is confirmed healthy — after that,
/// cleanup is the caller's responsibility.
struct DeployCleanup {
    deploy_layout: Option<RuntimeLayout>,
    worker: Option<WorkerConfig>,
    ctx: Option<VmBootContext>,
    deploy_tap: Option<String>,
    old_registry_status: AppStatus,
    registry_file: PathBuf,
    app: String,
    deploy_app: String,
    disarmed: bool,
}

impl DeployCleanup {
    fn from_deploy(
        app: String,
        deploy_layout: RuntimeLayout,
        old_registry_status: AppStatus,
        registry_file: PathBuf,
    ) -> Self {
        let deploy_app = format!("{app}-deploy");
        Self {
            deploy_layout: Some(deploy_layout),
            worker: None,
            ctx: None,
            deploy_tap: None,
            old_registry_status,
            registry_file,
            app,
            deploy_app,
            disarmed: false,
        }
    }

    fn set_worker(&mut self, worker: WorkerConfig) {
        self.worker = Some(worker);
    }

    fn booted(&mut self, ctx: VmBootContext) {
        self.ctx = Some(ctx);
    }

    fn set_deploy_tap(&mut self, tap: String) {
        self.deploy_tap = Some(tap);
    }

    fn disarm(&mut self) {
        self.deploy_layout = None;
        self.worker = None;
        self.ctx = None;
        self.deploy_tap = None;
        self.disarmed = true;
    }
}

impl Drop for DeployCleanup {
    fn drop(&mut self) {
        if let Some(worker) = self.worker.clone() {
            if let Some(ctx) = self.ctx.take() {
                let runner = SshRunner::new(worker.clone());
                let _ = runner.stop_firecracker(ctx.pid, &ctx.remote_runtime_dir);
            } else {
                let runner = SshRunner::new(worker);
                let _ = runner.remove_remote_runtime_for_app(&self.deploy_app);
            }
        }
        if let Some(worker) = self.worker.take()
            && let Some(tap) = self.deploy_tap.take()
        {
            let runner = SshRunner::new(worker);
            let _ = runner.delete_tap_interface(&tap);
        }
        if let Some(layout) = self.deploy_layout.take() {
            let _ = layout.remove();
        }
        if self.disarmed {
            return;
        }
        // Best-effort: restore previous registry status
        let old_status = self.old_registry_status.clone();
        if let Ok(mut registry) = Registry::load(&self.registry_file) {
            registry.apps.entry(self.app.clone()).and_modify(|entry| {
                entry.status = old_status.clone();
            });
            let _ = registry.save(&self.registry_file);
        }
    }
}

struct VmBootParams<'a> {
    kernel: &'a Path,
    rootfs: &'a Path,
    data: Option<&'a Path>,
    firecracker_bin: &'a str,
    tap: &'a str,
    boot_args: &'a str,
    vcpu: u8,
    memory_mib: u32,
}

fn boot_vm_on_worker(
    app: &str,
    worker: &WorkerConfig,
    worker_name: &str,
    params: &VmBootParams,
    layout: &RuntimeLayout,
) -> Result<VmBootContext> {
    let runner = SshRunner::new(worker.clone());
    let sp = progress::spinner("setting up runtime dirs");
    let remote_runtime = runner.create_runtime_dirs(app)?;
    sp.set_message("checking worker capabilities");
    runner.check_capabilities()?;
    sp.finish_and_clear();
    let kernel = prepare_boot_file(&runner, app, "kernel", params.kernel)?;
    let rootfs = prepare_boot_file(&runner, app, "rootfs", params.rootfs)?;
    let data = params
        .data
        .map(|data| prepare_boot_file(&runner, app, "data", data))
        .transpose()?;
    let sp = progress::spinner("ensuring TAP interface");
    runner.ensure_tap_interface(params.tap)?;
    sp.finish_and_clear();

    let api_socket_path = Path::new(&remote_runtime).join("firecracker.sock");
    let client = FirecrackerClient::new(&api_socket_path)?;

    let plan = BootPlan {
        machine_config: MachineConfig::new(params.vcpu, params.memory_mib)?,
        boot_source: BootSource::new(kernel, params.boot_args)?,
        rootfs: Drive::rootfs(rootfs, true)?,
        data_drive: data.map(Drive::data).transpose()?,
        network_interface: NetworkInterface::new("eth0", params.tap, None)?,
    };
    let requests = client.build_boot_requests(&plan)?;
    let start_request = client.start_instance()?;
    let mut all_requests = requests.clone();
    all_requests.push(start_request.clone());

    let state = RuntimeState::new(app.to_string(), client.api_socket_path().to_path_buf())
        .with_worker(worker_name.to_string())
        .with_remote_runtime_dir(PathBuf::from(&remote_runtime))
        .with_tap(params.tap.to_string())
        .with_status(RuntimeStatus::Starting)
        .with_status_message("boot plan prepared");
    state.save(&layout.state_file_path())?;

    log::info!(target: "deploy", "runtime: {}", layout.app_dir().display());
    log::info!(target: "deploy", "worker: {worker_name} ({})", worker.ssh_target());
    log::info!(target: "deploy", "remote runtime: {remote_runtime}");
    log::info!(target: "deploy", "api socket: {}", client.api_socket_path().display());
    log::info!(target: "deploy", "kernel: {}", plan.boot_source.kernel_image_path.display());
    log::info!(target: "deploy", "rootfs: {}", plan.rootfs.path_on_host.display());
    if let Some(data_drive) = &plan.data_drive {
        log::info!(target: "deploy", "data: {}", data_drive.path_on_host.display());
    }
    for request in &requests {
        log::info!(target: "deploy", "{} {}", request.method, request.path);
    }
    log::info!(target: "deploy", "{} {}", start_request.method, start_request.path);

    let remote_log_dir = Path::new(&remote_runtime).join("logs");
    let runner = SshRunner::new(worker.clone());
    let sp = progress::spinner("starting Firecracker");
    let pid = runner.start_firecracker(
        params.firecracker_bin,
        client.api_socket_path(),
        &remote_log_dir,
    )?;
    sp.finish_and_clear();

    RuntimeState::new(app.to_string(), client.api_socket_path().to_path_buf())
        .with_worker(worker_name.to_string())
        .with_remote_runtime_dir(PathBuf::from(&remote_runtime))
        .with_tap(params.tap.to_string())
        .with_pid(pid)
        .with_status(RuntimeStatus::Starting)
        .with_status_message("Firecracker started")
        .save(&layout.state_file_path())?;

    let sp = progress::spinner("configuring Firecracker VM");
    if let Err(e) = runner.send_firecracker_api_requests(client.api_socket_path(), &all_requests) {
        sp.finish_and_clear();
        let _ = runner.stop_firecracker(pid, &PathBuf::from(&remote_runtime));
        let _ = layout.remove();
        return Err(e).context("failed to configure new VM; cleaned up");
    }
    sp.finish_and_clear();

    RuntimeState::new(app.to_string(), client.api_socket_path().to_path_buf())
        .with_worker(worker_name.to_string())
        .with_remote_runtime_dir(PathBuf::from(&remote_runtime))
        .with_tap(params.tap.to_string())
        .with_pid(pid)
        .with_status(RuntimeStatus::Running)
        .with_status_message("booted")
        .save(&layout.state_file_path())?;

    log::info!(target: "deploy", "pid: {pid}");

    Ok(VmBootContext {
        pid,
        remote_runtime_dir: PathBuf::from(remote_runtime),
    })
}

pub fn init() -> Result<()> {
    let config = Config::default_from_env()?;
    config.create_dirs()?;
    config.save_to_env()?;

    let registry = Registry::load(&config.registry_file)?;
    registry.save(&config.registry_file)?;

    log::info!(target: "init", "initialized {}", config.registry_file.display());

    Ok(())
}

pub fn setup(options: SetupOptions) -> Result<()> {
    let config = load_config()?;
    let (worker_name, worker) = config.worker(options.worker.as_deref())?;
    let runner = SshRunner::new(worker.clone());

    log::info!(target: "setup", "worker {worker_name} ({})", worker.ssh_target());

    log::info!(target: "setup", "  prerequisites: installing...");
    runner.install_worker_prerequisites()?;
    log::info!(target: "setup", "  prerequisites: installed");

    log::info!(target: "setup", "ok");

    Ok(())
}

pub fn cleanup(options: CleanupOptions) -> Result<()> {
    let config = load_config()?;
    let (worker_name, worker) = config.worker(options.worker.as_deref())?;
    let runner = SshRunner::new(worker.clone());

    log::info!(target: "cleanup", "worker {worker_name} ({})", worker.ssh_target());

    if options.stale_taps {
        let preserve_taps = runtime_taps_for_worker(&config, worker_name)?;
        let removed = runner.cleanup_stale_tap_interfaces(&preserve_taps)?;
        if removed.is_empty() {
            log::info!(target: "cleanup", "no stale TAP interfaces found");
        } else {
            for tap in &removed {
                log::info!(target: "cleanup", "removed TAP {tap}");
            }
            log::info!(
                "cleanup: removed {} stale TAP interface{}",
                removed.len(),
                if removed.len() == 1 { "" } else { "s" }
            );
        }
    } else {
        log::warn!(target: "cleanup", "nothing selected");
    }

    Ok(())
}

fn runtime_taps_for_worker(config: &Config, worker_name: &str) -> Result<Vec<String>> {
    let entries = crate::runtime::list_runtime_entries(&config.runtime_dir)?;
    let mut taps = BTreeSet::new();
    for state in entries.values() {
        if state.worker.as_deref() == Some(worker_name)
            && let Some(tap) = &state.tap
        {
            taps.insert(tap.clone());
        }
    }

    Ok(taps.into_iter().collect())
}

pub fn ps() -> Result<()> {
    let config = load_config()?;
    let registry = Registry::load(&config.registry_file)?;
    let runtime_entries = crate::runtime::list_runtime_entries(&config.runtime_dir)?;
    let mut cleaned = 0u32;
    let mut warnings: Vec<String> = Vec::new();

    if registry.apps.is_empty() && runtime_entries.is_empty() {
        log::info!(target: "ps", "no apps");
        return Ok(());
    }

    for (name, state) in &runtime_entries {
        if is_process_stale(state) {
            let layout = RuntimeLayout::from_runtime_dir(&config.runtime_dir, name);
            if layout.remove().is_ok() {
                cleaned += 1;
            }
            continue;
        }

        let mut consistent = true;

        if let (Some(worker_name), Some(_)) = (&state.worker, &state.pid) {
            if let Ok((_, worker)) = config.worker(Some(worker_name)) {
                let runner = SshRunner::new(worker.clone());

                if let Some(remote_dir) = &state.remote_runtime_dir {
                    match runner.remote_dir_exists(remote_dir) {
                        Ok(true) => {}
                        Ok(false) => {
                            consistent = false;
                            warnings.push(format!(
                                "{name}: remote runtime directory {} does not exist on worker {worker_name}",
                                remote_dir.display()
                            ));
                        }
                        Err(e) => {
                            warnings.push(format!(
                                "{name}: could not verify remote runtime dir on worker {worker_name}: {e:#}"
                            ));
                        }
                    }
                }

                if let Some(pid) = state.pid {
                    match runner.check_remote_pid(pid) {
                        Ok(true) => {}
                        Ok(false) => {
                            consistent = false;
                            warnings.push(format!(
                                "{name}: Firecracker process {pid} is not running on worker {worker_name}"
                            ));
                        }
                        Err(e) => {
                            warnings.push(format!(
                                "{name}: could not verify remote PID {pid} on worker {worker_name}: {e:#}"
                            ));
                        }
                    }
                }
            } else {
                warnings.push(format!(
                    "{name}: worker {:?} is not defined in config",
                    state.worker.as_deref().unwrap_or("?")
                ));
            }
        }

        let worker = state.worker.as_deref().unwrap_or("-");
        let pid = state
            .pid
            .map(|p| p.to_string())
            .unwrap_or_else(|| "-".to_string());
        let runtime = state
            .remote_runtime_dir
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "-".to_string());
        let consistency = if consistent { "ok" } else { "MISMATCH" };
        println!(
            "{name}\t{:?}\t{worker}\t{pid}\t{runtime}\t{consistency}",
            state.status
        );
    }

    for (name, app) in &registry.apps {
        if !runtime_entries.contains_key(name) {
            println!("{name}\t{:?}\t{}", app.status, app.port);
        }
    }

    for warning in &warnings {
        log::warn!("{warning}");
    }

    if cleaned > 0 {
        log::warn!(
            target: "ps",
            "cleaned up {cleaned} stale runtime entr{}",
            if cleaned == 1 { "y" } else { "ies" }
        );
    }

    if !warnings.is_empty() {
        log::warn!(
            target: "ps",
            "found {} state {}",
            warnings.len(),
            if warnings.len() == 1 {
                "inconsistency"
            } else {
                "inconsistencies"
            }
        );
    }

    Ok(())
}

#[allow(dead_code)]
pub fn check_runtime_alignment(
    runtime_entries: &BTreeMap<String, RuntimeState>,
    config: &Config,
) -> Vec<String> {
    let mut misaligned = Vec::new();
    for (name, state) in runtime_entries {
        if let (Some(worker_name), Some(remote_dir)) = (&state.worker, &state.remote_runtime_dir)
            && let Ok((_, worker)) = config.worker(Some(worker_name))
        {
            let runner = SshRunner::new(worker.clone());
            match runner.remote_dir_exists(remote_dir) {
                Ok(true) => {}
                Ok(false) => {
                    log::warn!(
                        "{}: remote runtime dir {} does not exist on worker {worker_name}",
                        name,
                        remote_dir.display()
                    );
                    misaligned.push(name.clone());
                }
                Err(e) => {
                    log::warn!(
                        "{name}: could not verify remote runtime dir on worker {worker_name}: {e:#}"
                    );
                }
            }
        }
    }
    misaligned
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
        options
            .kernel
            .as_deref()
            .map(|kernel| runner.require_readable_file("kernel", kernel))
            .transpose()?
    };
    let rootfs = if options.skip_rootfs {
        None
    } else {
        options
            .rootfs
            .as_deref()
            .map(|rootfs| runner.require_readable_file("rootfs", rootfs))
            .transpose()?
    };
    let tap = if options.skip_tap {
        None
    } else {
        options
            .tap
            .as_deref()
            .map(|tap| runner.require_tap_interface(tap))
            .transpose()?
    };

    match runner.check_caddy() {
        Ok(_) => log::info!(target: "check", "caddy: installed and running"),
        Err(_) => {
            log::warn!(target: "check", "caddy: not found or not running on worker");
            log::info!(target: "check", "  run: v setup --worker {worker_name}");
        }
    }

    log::info!(target: "check", "worker: {worker_name} ({})", worker.ssh_target());
    if let Some(remote_runtime) = &remote_runtime {
        log::info!(target: "check", "remote runtime: {remote_runtime}");
        log::info!(
            target: "check",
            "api socket: {}",
            Path::new(remote_runtime).join("firecracker.sock").display()
        );
    }
    if let Some(kernel) = kernel {
        log::info!(target: "check", "kernel: {kernel}");
    }
    if let Some(rootfs) = rootfs {
        log::info!(target: "check", "rootfs: {rootfs}");
    }
    if let Some(tap) = tap {
        log::info!(target: "check", "tap: {tap}");
    }
    log::info!(target: "check", "ok");

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

pub fn deploy(options: DeployOptions) -> Result<()> {
    let config = load_config()?;
    let app = options.app.as_str();
    validate_remote_name("app", app)?;
    if options.rootfs.is_some() == options.artifact.is_some() {
        bail!("pass exactly one of --rootfs or --artifact");
    }
    if options.rootfs_size_mib == 0 {
        bail!("rootfs-size-mib must be greater than zero");
    }
    if options.data_size_mib == 0 {
        bail!("data-size-mib must be greater than zero");
    }

    let app_lock = LockFile::acquire(&config.locks_dir, &app_lock_name(app))
        .with_context(|| format!("lock acquisition failed; try: v unlock {app}"))?;
    let volume_lock = match LockFile::acquire(&config.locks_dir, &volume_lock_name(app)) {
        Ok(lock) => lock,
        Err(e) => {
            drop(app_lock);
            return Err(e).with_context(|| format!("lock acquisition failed; try: v unlock {app}"));
        }
    };
    let _app_lock = app_lock;
    let _volume_lock = volume_lock;

    let mut registry = Registry::load(&config.registry_file)?;
    let old_port = registry
        .apps
        .get(app)
        .map(|entry| entry.port)
        .unwrap_or(8080u16);
    log::info!(
        target: "deploy",
        "port {} (old VM port, reused for new VM)",
        old_port
    );

    let is_deploying = registry
        .apps
        .get(app)
        .is_some_and(|e| e.status == AppStatus::Deploying);
    if is_deploying {
        let old_layout = RuntimeLayout::from_runtime_dir(&config.runtime_dir, app);
        let deploy_layout =
            RuntimeLayout::from_runtime_dir(&config.runtime_dir, format!("{app}-deploy"));
        if deploy_layout.state_file_path().exists() {
            let deploy_state = RuntimeState::load(&deploy_layout.state_file_path()).ok();
            let deploy_is_stale = deploy_state
                .as_ref()
                .map(|state| {
                    state.status == RuntimeStatus::Stopped
                        || state.pid.is_none()
                        || state.worker.is_none()
                })
                .unwrap_or(true);
            if deploy_is_stale {
                log::warn!(
                    target: "deploy",
                    "previous deploy was interrupted; cleaning up stale deploy resources"
                );
                if let Some(ref deploy_state) = deploy_state
                    && let Some(ref worker_name) = deploy_state.worker
                    && let Ok((_, worker)) = config.worker(Some(worker_name))
                {
                    let runner = SshRunner::new(worker.clone());
                    if let Some(ref remote_runtime_dir) = deploy_state.remote_runtime_dir {
                        let _ = runner.remove_remote_runtime_dir(Path::new(remote_runtime_dir));
                    } else {
                        let _ = runner.remove_remote_runtime_for_app(&format!("{app}-deploy"));
                    }
                    if let Some(ref tap) = deploy_state.tap {
                        let _ = runner.delete_tap_interface(tap);
                    }
                }
                let _ = deploy_layout.remove();
            } else {
                bail!("{app}: deploy is already in progress");
            }
        }
        log::warn!(target: "deploy", "previous deploy was interrupted; resetting status and retrying");
        let recovered_status = RuntimeState::load(&old_layout.state_file_path())
            .ok()
            .and_then(|state| match state.status {
                RuntimeStatus::Running => Some(AppStatus::Running),
                RuntimeStatus::Stopped => Some(AppStatus::Stopped),
                _ => None,
            })
            .unwrap_or(AppStatus::Stopped);
        registry.apps.entry(app.to_string()).and_modify(|e| {
            e.status = recovered_status;
        });
        registry.save(&config.registry_file)?;
    }

    let old_status = registry
        .apps
        .get(app)
        .map(|e| e.status.clone())
        .unwrap_or(AppStatus::Stopped);

    let deploy_app = format!("{app}-deploy");
    let deploy_layout = RuntimeLayout::from_runtime_dir(&config.runtime_dir, &deploy_app);
    deploy_layout.create_dirs()?;

    let old_layout = RuntimeLayout::from_runtime_dir(&config.runtime_dir, app);
    let old_state = RuntimeState::load(&old_layout.state_file_path()).ok();
    let has_old_vm = old_state.is_some();

    if options.dry_run {
        if let Some(artifact) = &options.artifact
            && !artifact.is_file()
        {
            bail!("local artifact file does not exist: {}", artifact.display());
        }
        if let Some(rootfs) = &options.rootfs
            && !rootfs.is_file()
            && !is_remote_boot_path(rootfs)
        {
            bail!(
                "local rootfs file does not exist: {}; pass an existing local file or a worker-side absolute/XDG path",
                rootfs.display()
            );
        }
        if let Some(data) = &options.data
            && !data.is_file()
            && !is_remote_boot_path(data)
        {
            bail!(
                "local data file does not exist: {}; pass an existing local file or a worker-side absolute/XDG path",
                data.display()
            );
        }
        if !options.kernel.is_file() && !is_remote_boot_path(&options.kernel) {
            bail!(
                "local kernel file does not exist: {}; pass an existing local file or a worker-side absolute/XDG path",
                options.kernel.display()
            );
        }

        if let Some((worker_name, worker)) =
            resolve_worker(&config, options.worker.as_deref(), true)?
        {
            let runner = SshRunner::new(worker.clone());
            log::info!(
                target: "deploy",
                "dry-run worker: {worker_name} ({})",
                worker.ssh_target()
            );
            runner.check_capabilities()?;
            log::info!(target: "deploy", "  worker capabilities: ok");

            if !options.kernel.is_file() {
                let resolved = runner.require_readable_file("kernel", &options.kernel)?;
                log::info!(target: "deploy", "  kernel validated: {resolved}");
            }
            if let Some(ref rootfs) = options.rootfs
                && !rootfs.is_file()
            {
                let resolved = runner.require_readable_file("rootfs", rootfs)?;
                log::info!(target: "deploy", "  rootfs validated: {resolved}");
            }
            if let Some(ref data) = options.data
                && !data.is_file()
            {
                let resolved = runner.require_readable_file("data", data)?;
                log::info!(target: "deploy", "  data validated: {resolved}");
            }
        } else {
            log::info!(target: "deploy", "dry-run (no worker selected; remote checks skipped)");
        }

        deploy_layout.remove()?;
        log::info!(target: "deploy", "dry-run for {app}");
        if let Some(rootfs) = &options.rootfs {
            log::info!(target: "deploy", "  new rootfs: {}", rootfs.display());
        }
        if let Some(artifact) = &options.artifact {
            log::info!(target: "deploy", "  artifact: {}", artifact.display());
            log::info!(target: "deploy", "  would upload artifact to worker");
            log::info!(
                target: "deploy",
                "  would materialize rootfs.ext4 on worker ({} MiB)",
                options.rootfs_size_mib
            );
            log::info!(
                target: "deploy",
                "  would ensure data.ext4 on worker ({} MiB)",
                options.data_size_mib
            );
        }
        if let Some(data) = &options.data {
            log::info!(target: "deploy", "  data: {}", data.display());
        }
        log::info!(target: "deploy", "  kernel: {}", options.kernel.display());
        log::info!(target: "deploy", "  tap: {}", options.tap);
        log::info!(target: "deploy", "  vcpu: {}", options.vcpu);
        log::info!(target: "deploy", "  memory_mib: {}", options.memory_mib);
        log::info!(target: "deploy", "  health check host: {}", options.health_check_host);
        if let Some(ref path) = options.health_check_path {
            log::info!(target: "deploy", "  health check path: {path}");
        }
        log::info!(
            target: "deploy",
            "  health check: {} retries, {}s interval, {}s timeout",
            options.health_check_retries,
            options.health_check_interval_secs,
            options.health_check_timeout_secs,
        );
        if let Some(ref domain) = options.domain {
            let proxy_target = format!("{}:{}", options.health_check_host, old_port);
            log::info!(target: "deploy", "  would update reverse proxy: {domain} → {proxy_target}");
            log::info!(target: "deploy", "  would reload Caddy");
        } else {
            log::info!(target: "deploy", "  (no --domain set; proxy routing is manual)");
        }
        return Ok(());
    }

    // deploy_layout is cleaned up on drop if anything fails from here
    let mut cleanup = DeployCleanup::from_deploy(
        app.to_string(),
        deploy_layout.clone(),
        old_status,
        config.registry_file.clone(),
    );

    log::info!(target: "deploy", "starting deploy for {app}");

    let (worker_name, worker) =
        resolve_worker(&config, options.worker.as_deref(), options.dry_run)?
            .context("worker is required for deploy")?;
    cleanup.set_worker(worker.clone());
    log::info!(
        target: "deploy",
        "resolved worker {worker_name} ({})",
        worker.ssh_target()
    );

    // Mark deploying — Drop guard reverts this on failure
    log::info!(target: "deploy", "marking registry status as deploying");
    registry.apps.insert(
        app.to_string(),
        crate::registry::App {
            current_image: registry.apps.get(app).and_then(|e| e.current_image.clone()),
            previous_image: registry
                .apps
                .get(app)
                .and_then(|e| e.previous_image.clone()),
            volume_path: registry
                .apps
                .get(app)
                .map(|e| e.volume_path.clone())
                .unwrap_or_else(|| config.volumes_dir.join(app)),
            port: old_port,
            status: AppStatus::Deploying,
        },
    );
    registry.save(&config.registry_file)?;

    let worker_name = worker_name.to_string();
    let firecracker_bin = options.firecracker_bin.to_string_lossy().to_string();
    let deploy_tap = format!(
        "t-{ts:012x}",
        ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
            % 0xffffffffffff,
    );
    cleanup.set_deploy_tap(deploy_tap.clone());

    let mut materialized_data: Option<PathBuf> = None;
    let rootfs = if let Some(rootfs) = &options.rootfs {
        log::info!(target: "deploy", "using provided rootfs {}", rootfs.display());
        rootfs.clone()
    } else {
        let artifact = options
            .artifact
            .as_ref()
            .context("artifact is required when rootfs is not set")?;
        log::info!(target: "deploy", "uploading artifact for {app}");
        let runner = SshRunner::new(worker.clone());
        let remote_artifact = if artifact.is_file() {
            runner
                .upload_artifact_tar(app, artifact)
                .context(format!("failed to upload artifact {}", artifact.display()))?
        } else if is_remote_boot_path(artifact) {
            runner
                .require_readable_file("artifact", artifact)
                .context(format!(
                    "failed to validate remote artifact {}",
                    artifact.display()
                ))?
        } else {
            bail!(
                "local artifact file does not exist: {}; pass an existing local file or a worker-side absolute/XDG path",
                artifact.display()
            );
        };
        let sp = progress::spinner("materializing rootfs on worker");
        let artifact = runner.materialize_rootfs_from_artifact(
            app,
            Path::new(&remote_artifact),
            options.rootfs_size_mib,
            options.data_size_mib,
            old_port,
            "/sbin/init",
        )?;
        sp.finish_and_clear();
        log::info!(target: "deploy", "worker rootfs {}", artifact.rootfs.display());
        log::info!(target: "deploy", "worker data {}", artifact.data.display());
        materialized_data = Some(artifact.data);
        artifact.rootfs
    };
    let data = options.data.clone().or(materialized_data);

    log::info!(target: "deploy", "booting new VM for {app}");

    let params = VmBootParams {
        kernel: &options.kernel,
        rootfs: &rootfs,
        data: data.as_deref(),
        firecracker_bin: &firecracker_bin,
        tap: &deploy_tap,
        boot_args: &options.boot_args,
        vcpu: options.vcpu,
        memory_mib: options.memory_mib,
    };
    let ctx = boot_vm_on_worker(&deploy_app, worker, &worker_name, &params, &deploy_layout)?;
    cleanup.booted(ctx.clone());

    log::info!(target: "deploy", "new VM booted (pid: {})", ctx.pid);

    // Health checks
    log::info!(target: "deploy", "starting health checks");
    let mut last_error: Option<anyhow::Error> = None;
    let mut health_ok = options.skip_health_check;
    if health_ok {
        log::info!(target: "deploy", "health check skipped (--skip-health-check)");
    } else {
        let hc_spinner = progress::spinner(&format!(
            "health check ({}:{})",
            options.health_check_host, old_port
        ));
        for i in 0..options.health_check_retries {
            hc_spinner.set_message(format!(
                "health check attempt {}/{}",
                i + 1,
                options.health_check_retries
            ));
            if i > 0 {
                thread::sleep(Duration::from_secs(
                    options.health_check_interval_secs as u64,
                ));
            }
            let runner = SshRunner::new(worker.clone());
            let result = if let Some(ref path) = options.health_check_path {
                runner.http_health_check(
                    &options.health_check_host,
                    old_port,
                    path,
                    options.health_check_timeout_secs,
                )
            } else {
                runner.health_check(
                    &options.health_check_host,
                    old_port,
                    options.health_check_timeout_secs,
                )
            };
            match result {
                Ok(()) => {
                    log::info!(
                        target: "deploy",
                        "health check passed (attempt {}/{})",
                        i + 1,
                        options.health_check_retries
                    );
                    health_ok = true;
                    last_error = None;
                    break;
                }
                Err(e) if i + 1 == options.health_check_retries => {
                    last_error = Some(e);
                }
                Err(e) => {
                    log::warn!(
                        target: "deploy",
                        "health check attempt {}/{} failed: {e:#}",
                        i + 1,
                        options.health_check_retries
                    );
                    last_error = Some(e);
                }
            }
        }
        if health_ok {
            hc_spinner.finish_with_message("health check passed");
        } else {
            hc_spinner.finish_and_clear();
        }
    }
    if !health_ok {
        log::error!(target: "deploy", "health check failed; remote VM logs follow");
        let runner = SshRunner::new(worker.clone());
        if let Err(e) = runner.stream_logs(&ctx.remote_runtime_dir.join("logs")) {
            log::error!(target: "deploy", "failed to read remote VM logs: {e:#}");
        }
        if let Some(ref e) = last_error {
            log::error!(target: "deploy", "last health check error: {e:#}");
        }
        bail!(
            "deploy: health check failed after {} retries",
            options.health_check_retries
        );
    }
    log::info!(target: "deploy", "health checks completed");

    // New VM confirmed healthy — keep it even if later steps fail
    cleanup.disarm();
    log::info!(target: "deploy", "new VM confirmed healthy, disarming cleanup guard");

    // Switch traffic
    log::info!(target: "deploy", "switching traffic for {app}");

    let runner = SshRunner::new(worker.clone());
    if let Some(domain) = options.domain.as_ref() {
        let sp = progress::spinner(&format!("updating proxy {domain}"));
        if let Err(e) =
            runner.update_caddy_config(app, domain, &options.health_check_host, old_port)
        {
            sp.finish_and_clear();
            log::error!(target: "deploy", "failed to update Caddy config: {e:#}");
        } else if let Err(e) = runner.reload_caddy() {
            sp.finish_and_clear();
            log::error!(target: "deploy", "failed to reload Caddy: {e:#}");
        } else {
            sp.finish_and_clear();
            log::info!(target: "deploy", "Caddy reloaded");
        }
    } else {
        log::info!(target: "deploy", "update your reverse proxy to route traffic for {app} to the new VM");
        log::info!(target: "deploy", "reload your proxy (e.g. `systemctl reload caddy`)");
    }
    log::info!(target: "deploy", "traffic switch completed");

    // Drain old VM
    if has_old_vm {
        let sp = progress::spinner(&format!("draining old VM ({}s)", options.drain_seconds));
        thread::sleep(Duration::from_secs(options.drain_seconds as u64));
        sp.finish_and_clear();

        if let Some(ref old_state) = old_state
            && let (Some(pid), Some(remote_runtime_dir)) =
                (old_state.pid, old_state.remote_runtime_dir.as_ref())
            && let Some(ref old_worker_name) = old_state.worker
            && let Ok((_, old_worker)) = config.worker(Some(old_worker_name))
        {
            let sp = progress::spinner(&format!("stopping old VM (pid: {pid})"));
            let old_runner = SshRunner::new(old_worker.clone());
            old_runner
                .stop_firecracker(pid, remote_runtime_dir)
                .context("failed to stop old VM; new VM is still healthy and running")?;
            sp.finish_and_clear();
        }
        old_layout.remove()?;

        // Delete old TAP device (the original one, not deploy-*)
        let old_runner = SshRunner::new(worker.clone());
        let _ = old_runner.delete_tap_interface(&options.tap);
    }
    log::info!(target: "deploy", "drain completed");

    // Rename remote runtime: {app}-deploy → {app}
    // Must happen AFTER old VM is stopped so the destination directory is free.
    let sp = progress::spinner(&format!("renaming runtime {deploy_app} → {app}"));
    let deploy_remote_dir = ctx.remote_runtime_dir.clone();
    let remote_name_pattern = format!("/{deploy_app}");
    let new_dir = PathBuf::from(
        deploy_remote_dir
            .to_string_lossy()
            .replace(&remote_name_pattern, &format!("/{app}")),
    );
    let (primary_remote_dir, rename_failed) =
        match rename_runtime_dir_with_retries(worker, &deploy_remote_dir, &new_dir) {
            Ok(()) => {
                sp.finish_with_message(format!("renamed {deploy_app} → {app}"));
                (new_dir, false)
            }
            Err(e) => {
                sp.finish_and_clear();
                log::error!(
                    target: "deploy",
                    "could not rename remote runtime (keeping {}): {e:#}",
                    deploy_remote_dir.display()
                );
                (deploy_remote_dir.clone(), true)
            }
        };

    // Swap runtime state: {app}-deploy → {app}
    log::info!(target: "deploy", "swapping runtime state {deploy_app} → {app}");
    let deploy_state = RuntimeState::load(&deploy_layout.state_file_path())?;
    let new_layout = RuntimeLayout::from_runtime_dir(&config.runtime_dir, app);
    old_layout.remove()?;
    new_layout.create_dirs()?;
    let primary_api_socket = primary_remote_dir.join("firecracker.sock");
    let status_message = if rename_failed {
        "deployed (runtime rename failed)"
    } else {
        "deployed"
    };
    RuntimeState::new(app.to_string(), primary_api_socket)
        .with_pid(deploy_state.pid.unwrap_or(ctx.pid))
        .with_worker(
            deploy_state
                .worker
                .clone()
                .unwrap_or_else(|| worker_name.clone()),
        )
        .with_remote_runtime_dir(primary_remote_dir)
        .with_tap(deploy_state.tap.clone().unwrap_or(deploy_tap))
        .with_status(RuntimeStatus::Running)
        .with_status_message(status_message)
        .save(&new_layout.state_file_path())?;
    deploy_layout.remove()?;
    log::info!(target: "deploy", "runtime state swapped");

    // Commit registry
    log::info!(target: "deploy", "committing registry");
    let old_current = registry
        .apps
        .get(app)
        .and_then(|entry| entry.current_image.clone());
    registry.apps.entry(app.to_string()).and_modify(|entry| {
        entry.previous_image = old_current;
        entry.current_image = Some(rootfs.clone());
        if let Some(data) = data.clone() {
            entry.volume_path = data;
        }
        entry.status = AppStatus::Running;
    });
    registry.save(&config.registry_file)?;
    log::info!(target: "deploy", "registry committed");

    log::info!(target: "deploy", "{app} deployed successfully");

    Ok(())
}

fn rename_runtime_dir_with_retries(
    worker: &WorkerConfig,
    old_path: &Path,
    new_path: &Path,
) -> Result<()> {
    let runner = SshRunner::new(worker.clone());
    let mut last_error = None;
    for attempt in 1..=3u32 {
        match runner.rename_runtime_dir(old_path, new_path) {
            Ok(()) => return Ok(()),
            Err(e) => {
                last_error = Some(e);
                if attempt < 3 {
                    log::warn!(
                        target: "deploy",
                        "remote rename attempt {}/3 failed, retrying...",
                        attempt
                    );
                    thread::sleep(Duration::from_millis(500 * attempt as u64));
                }
            }
        }
    }
    Err(last_error.unwrap())
}

pub fn unlock(app: &str) -> Result<()> {
    let config = load_config()?;
    let app_lock_path = config
        .locks_dir
        .join(format!("{}.lock", app_lock_name(app)));
    let volume_lock_path = config
        .locks_dir
        .join(format!("{}.lock", volume_lock_name(app)));

    let mut removed = false;
    for path in [&app_lock_path, &volume_lock_path] {
        let info = crate::lock::read_lock_info(path);
        match fs::remove_file(path) {
            Ok(()) => {
                let detail = match (info.pid, info.timestamp) {
                    (Some(pid), Some(_)) => {
                        format!(" (held by pid {pid}, {})", info.display_age())
                    }
                    _ => String::new(),
                };
                log::info!(target: "unlock", "removed {}{detail}", path.display());
                removed = true;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                log::warn!(target: "unlock", "failed to remove {}: {e}", path.display());
            }
        }
    }

    if !removed {
        log::info!(target: "unlock", "no lock files found for {app}");
    }

    Ok(())
}

pub fn rollback(app: &str) -> Result<()> {
    let config = load_config()?;
    let _app_lock = LockFile::acquire(&config.locks_dir, &app_lock_name(app))?;

    log::info!(target: "rollback", "rolling back {app} is not implemented yet");

    Ok(())
}

pub fn stop(app: &str) -> Result<()> {
    let config = load_config()?;
    let _app_lock = LockFile::acquire(&config.locks_dir, &app_lock_name(app))?;
    let layout = RuntimeLayout::from_runtime_dir(&config.runtime_dir, app);
    let mut registry = Registry::load(&config.registry_file)?;

    let state = match RuntimeState::load(&layout.state_file_path()) {
        Ok(state) => state,
        Err(error) if is_not_found_error(&error) => {
            log::info!(target: "stop", "{app} is not running");
            return Ok(());
        }
        Err(error) => return Err(error),
    };

    let mut worker_for_proxy: Option<WorkerConfig> = None;

    let stopped_pid = state.pid;
    if let Some(worker_name) = &state.worker {
        if let (Some(pid), Some(remote_runtime_dir)) =
            (state.pid, state.remote_runtime_dir.as_ref())
        {
            let (_, worker) = config.worker(Some(worker_name))?;
            worker_for_proxy = Some(worker.clone());
            let runner = SshRunner::new(worker.clone());
            runner.stop_firecracker(pid, remote_runtime_dir)?;
        }
    } else if let Some(pid) = state.pid {
        terminate_process(pid)?;
    }

    layout.remove()?;

    match stopped_pid {
        Some(pid) => log::info!(target: "stop", "stopped {app} pid {pid}"),
        None => log::info!(target: "stop", "stopped {app}"),
    }

    if let Some(ref worker) = worker_for_proxy {
        let runner = SshRunner::new(worker.clone());
        let _ = runner.remove_caddy_config(app);
        let _ = runner.reload_caddy();
    }

    registry.apps.entry(app.to_string()).and_modify(|entry| {
        entry.status = AppStatus::Stopped;
    });
    registry.save(&config.registry_file)?;

    Ok(())
}

pub fn logs(app: &str) -> Result<()> {
    let config = load_config()?;
    let layout = RuntimeLayout::from_runtime_dir(&config.runtime_dir, app);

    let state = match RuntimeState::load(&layout.state_file_path()) {
        Ok(state) => state,
        Err(error) if is_not_found_error(&error) => {
            log::info!(target: "logs", "{app} is not running");
            return Ok(());
        }
        Err(error) => return Err(error),
    };

    if let Some(worker_name) = &state.worker {
        let remote_runtime_dir = state
            .remote_runtime_dir
            .as_ref()
            .context("remote runtime directory not found in state")?;
        let (_, worker) = config.worker(Some(worker_name))?;
        let runner = SshRunner::new(worker.clone());
        runner.stream_logs(&remote_runtime_dir.join("logs"))?;
    } else {
        let log_dir = layout.log_dir();
        if log_dir.is_dir() {
            for entry in fs::read_dir(&log_dir)? {
                let entry = entry?;
                if entry.file_type()?.is_file() {
                    let path = entry.path();
                    if let Some(filename) = path.file_name() {
                        println!("=== {} ===", filename.to_string_lossy());
                    }
                    let content = fs::read_to_string(&path)?;
                    print!("{content}");
                }
            }
        } else {
            log::info!(target: "logs", "no logs found for {app}");
        }
    }

    Ok(())
}

fn is_process_stale(state: &RuntimeState) -> bool {
    if state.worker.is_some() {
        return false;
    }
    if let Some(pid) = state.pid {
        let status = Command::new("kill")
            .arg("-0")
            .arg(pid.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        return !matches!(status, Ok(s) if s.success());
    }
    false
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

use crate::config::Config;
use crate::config::WorkerConfig;
use crate::firecracker::{
    BootPlan, BootSource, Drive, FirecrackerClient, MachineConfig, NetworkInterface,
};
use crate::lock::{LockFile, app_lock_name, volume_lock_name};
use crate::registry::{AppStatus, Registry};
use crate::runtime::{RuntimeLayout, RuntimeState, RuntimeStatus};
use crate::ssh::{SshRunner, remote_runtime_dir_display, validate_remote_name};
use anyhow::{Context, Result, bail};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
    pub rootfs: PathBuf,
    pub firecracker_bin: PathBuf,
    pub tap: String,
    pub boot_args: String,
    pub vcpu: u8,
    pub memory_mib: u32,
    pub health_check_host: String,
    pub health_check_path: Option<String>,
    pub health_check_retries: u32,
    pub health_check_interval_secs: u32,
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
    old_registry_status: AppStatus,
    registry_file: PathBuf,
    app: String,
}

impl DeployCleanup {
    fn from_deploy(
        app: String,
        deploy_layout: RuntimeLayout,
        old_registry_status: AppStatus,
        registry_file: PathBuf,
    ) -> Self {
        Self {
            deploy_layout: Some(deploy_layout),
            worker: None,
            ctx: None,
            old_registry_status,
            registry_file,
            app,
        }
    }

    fn set_worker(&mut self, worker: WorkerConfig) {
        self.worker = Some(worker);
    }

    fn booted(&mut self, ctx: VmBootContext) {
        self.ctx = Some(ctx);
    }

    fn disarm(&mut self) {
        self.deploy_layout = None;
        self.worker = None;
        self.ctx = None;
    }
}

impl Drop for DeployCleanup {
    fn drop(&mut self) {
        if let Some(ctx) = self.ctx.take()
            && let Some(worker) = self.worker.take()
        {
            let runner = SshRunner::new(worker);
            let _ = runner.stop_firecracker(ctx.pid, &ctx.remote_runtime_dir);
        }
        if let Some(layout) = self.deploy_layout.take() {
            let _ = layout.remove();
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
    let remote_runtime = runner.create_runtime_dirs(app)?;
    runner.check_capabilities()?;
    let kernel = prepare_boot_file(&runner, app, "kernel", params.kernel)?;
    let rootfs = prepare_boot_file(&runner, app, "rootfs", params.rootfs)?;
    runner.ensure_tap_interface(params.tap)?;

    let api_socket_path = Path::new(&remote_runtime).join("firecracker.sock");
    let client = FirecrackerClient::new(&api_socket_path)?;

    let plan = BootPlan {
        machine_config: MachineConfig::new(params.vcpu, params.memory_mib)?,
        boot_source: BootSource::new(kernel, params.boot_args)?,
        rootfs: Drive::rootfs(rootfs, true)?,
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

    println!("runtime: {}", layout.app_dir().display());
    println!("worker: {worker_name} ({})", worker.ssh_target());
    println!("remote runtime: {remote_runtime}");
    println!("api socket: {}", client.api_socket_path().display());
    println!("kernel: {}", plan.boot_source.kernel_image_path.display());
    println!("rootfs: {}", plan.rootfs.path_on_host.display());
    for request in &requests {
        println!("{} {}", request.method, request.path);
    }
    println!("{} {}", start_request.method, start_request.path);

    let remote_log_dir = Path::new(&remote_runtime).join("logs");
    let runner = SshRunner::new(worker.clone());
    let pid = runner.start_firecracker(
        params.firecracker_bin,
        client.api_socket_path(),
        &remote_log_dir,
    )?;

    RuntimeState::new(app.to_string(), client.api_socket_path().to_path_buf())
        .with_worker(worker_name.to_string())
        .with_remote_runtime_dir(PathBuf::from(&remote_runtime))
        .with_tap(params.tap.to_string())
        .with_pid(pid)
        .with_status(RuntimeStatus::Starting)
        .with_status_message("Firecracker started")
        .save(&layout.state_file_path())?;

    if let Err(e) = runner.send_firecracker_api_requests(client.api_socket_path(), &all_requests) {
        let _ = runner.stop_firecracker(pid, &PathBuf::from(&remote_runtime));
        let _ = layout.remove();
        return Err(e).context("failed to configure new VM; cleaned up");
    }

    RuntimeState::new(app.to_string(), client.api_socket_path().to_path_buf())
        .with_worker(worker_name.to_string())
        .with_remote_runtime_dir(PathBuf::from(&remote_runtime))
        .with_tap(params.tap.to_string())
        .with_pid(pid)
        .with_status(RuntimeStatus::Running)
        .with_status_message("booted")
        .save(&layout.state_file_path())?;

    println!("pid: {pid}");

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

    println!("initialized {}", config.registry_file.display());

    Ok(())
}

pub fn setup(options: SetupOptions) -> Result<()> {
    let config = load_config()?;
    let (worker_name, worker) = config.worker(options.worker.as_deref())?;
    let runner = SshRunner::new(worker.clone());

    println!("v setup: worker {worker_name} ({})", worker.ssh_target());
    println!();

    println!("  worker prerequisites: installing...");
    runner.install_worker_prerequisites()?;
    println!("  worker prerequisites: installed");

    println!();
    println!("ok");

    Ok(())
}

pub fn cleanup(options: CleanupOptions) -> Result<()> {
    let config = load_config()?;
    let (worker_name, worker) = config.worker(options.worker.as_deref())?;
    let runner = SshRunner::new(worker.clone());

    println!("cleanup: worker {worker_name} ({})", worker.ssh_target());

    if options.stale_taps {
        let preserve_taps = runtime_taps_for_worker(&config, worker_name)?;
        let removed = runner.cleanup_stale_tap_interfaces(&preserve_taps)?;
        if removed.is_empty() {
            println!("cleanup: no stale TAP interfaces found");
        } else {
            for tap in &removed {
                println!("cleanup: removed TAP {tap}");
            }
            println!(
                "cleanup: removed {} stale TAP interface{}",
                removed.len(),
                if removed.len() == 1 { "" } else { "s" }
            );
        }
    } else {
        println!("cleanup: nothing selected");
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

    if registry.apps.is_empty() && runtime_entries.is_empty() {
        println!("no apps");
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
        println!("{name}\t{:?}\t{worker}\t{pid}\t{runtime}", state.status);
    }

    for (name, app) in &registry.apps {
        if !runtime_entries.contains_key(name) {
            println!("{name}\t{:?}\t{}", app.status, app.port);
        }
    }

    if cleaned > 0 {
        println!(
            "cleaned up {cleaned} stale runtime entr{}",
            if cleaned == 1 { "y" } else { "ies" }
        );
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
    let remote_runtime = worker.map(|_| remote_runtime_dir_display(app));

    let plan = BootPlan {
        machine_config: MachineConfig::new(options.vcpu, options.memory_mib)?,
        boot_source: BootSource::new(&options.kernel, &options.boot_args)?,
        rootfs: Drive::rootfs(&options.rootfs, true)?,
        network_interface: NetworkInterface::new("eth0", &options.tap, None)?,
    };

    let api_socket_path = remote_runtime
        .as_deref()
        .map(|runtime| Path::new(runtime).join("firecracker.sock"))
        .unwrap_or_else(|| layout.api_socket_path());

    let client = FirecrackerClient::new(&api_socket_path)?;
    let requests = client.build_boot_requests(&plan)?;
    let start_request = client.start_instance()?;
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
    for request in &requests {
        println!("{} {}", request.method, request.path);
    }
    println!("{} {}", start_request.method, start_request.path);

    if !options.dry_run {
        let (worker_name, worker) = worker.expect("worker is required for non dry-run");
        let firecracker_bin = options.firecracker_bin.to_string_lossy().to_string();
        let params = VmBootParams {
            kernel: &options.kernel,
            rootfs: &options.rootfs,
            firecracker_bin: &firecracker_bin,
            tap: &options.tap,
            boot_args: &options.boot_args,
            vcpu: options.vcpu,
            memory_mib: options.memory_mib,
        };
        boot_vm_on_worker(app, worker, worker_name, &params, &layout)?;
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

    match runner.check_caddy() {
        Ok(_) => println!("caddy: installed and running"),
        Err(_) => {
            println!("caddy: not found or not running on worker");
            println!("  run: v setup --worker {worker_name}");
        }
    }

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

pub fn deploy(options: DeployOptions) -> Result<()> {
    let config = load_config()?;
    let app = options.app.as_str();
    validate_remote_name("app", app)?;

    let app_lock =
        LockFile::acquire(&config.locks_dir, &app_lock_name(app)).with_context(|| {
            format!(
                "stale lock? try: rm {}/{}.lock",
                config.locks_dir.display(),
                app_lock_name(app)
            )
        })?;
    let volume_lock = match LockFile::acquire(&config.locks_dir, &volume_lock_name(app)) {
        Ok(lock) => lock,
        Err(e) => {
            drop(app_lock);
            return Err(e).with_context(|| {
                format!(
                    "stale lock? try: rm {}/{}.lock",
                    config.locks_dir.display(),
                    volume_lock_name(app)
                )
            });
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

    let is_deploying = registry
        .apps
        .get(app)
        .is_some_and(|e| e.status == AppStatus::Deploying);
    if is_deploying {
        let old_layout = RuntimeLayout::from_runtime_dir(&config.runtime_dir, app);
        let deploy_layout =
            RuntimeLayout::from_runtime_dir(&config.runtime_dir, format!("{app}-deploy"));
        if deploy_layout.state_file_path().exists() {
            bail!("{app}: deploy is already in progress");
        }
        println!("deploy: previous deploy was interrupted; resetting status and retrying");
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
        deploy_layout.remove()?;
        println!("deploy: dry-run for {app}");
        println!("  new rootfs: {}", options.rootfs.display());
        println!("  kernel: {}", options.kernel.display());
        println!("  tap: {}", options.tap);
        println!("  vcpu: {}", options.vcpu);
        println!("  memory_mib: {}", options.memory_mib);
        println!("  health check host: {}", options.health_check_host);
        if let Some(ref domain) = options.domain {
            let proxy_target = format!("{}:{}", options.health_check_host, old_port);
            println!("  would update reverse proxy: {domain} → {proxy_target}");
            println!("  would reload Caddy");
        } else {
            println!("  (no --domain set; proxy routing is manual)");
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

    let (worker_name, worker) =
        resolve_worker(&config, options.worker.as_deref(), options.dry_run)?
            .context("worker is required for deploy")?;
    cleanup.set_worker(worker.clone());

    // Mark deploying — Drop guard reverts this on failure
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
        "t-{ts:08x}",
        ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            % 0xffffffff,
    );

    println!("deploy: booting new VM for {app}");

    let params = VmBootParams {
        kernel: &options.kernel,
        rootfs: &options.rootfs,
        firecracker_bin: &firecracker_bin,
        tap: &deploy_tap,
        boot_args: &options.boot_args,
        vcpu: options.vcpu,
        memory_mib: options.memory_mib,
    };
    let ctx = boot_vm_on_worker(&deploy_app, worker, &worker_name, &params, &deploy_layout)?;
    cleanup.booted(ctx.clone());

    println!("deploy: new VM booted (pid: {})", ctx.pid);

    // Health checks
    println!(
        "deploy: running health checks against {}:{}...",
        options.health_check_host, old_port
    );
    let mut health_ok = options.skip_health_check;
    if health_ok {
        println!("deploy: health check skipped (--skip-health-check)");
    } else {
        println!(
            "deploy: running health checks against {}:{}...",
            options.health_check_host, old_port
        );
        for i in 0..options.health_check_retries {
            if i > 0 {
                thread::sleep(Duration::from_secs(
                    options.health_check_interval_secs as u64,
                ));
            }
            let runner = SshRunner::new(worker.clone());
            let result = if let Some(ref path) = options.health_check_path {
                runner.http_health_check(&options.health_check_host, old_port, path)
            } else {
                runner.health_check(&options.health_check_host, old_port)
            };
            match result {
                Ok(()) => {
                    println!(
                        "deploy: health check passed (attempt {}/{})",
                        i + 1,
                        options.health_check_retries
                    );
                    health_ok = true;
                    break;
                }
                Err(_) if i + 1 == options.health_check_retries => {}
                Err(_) => {}
            }
        }
    }
    if !health_ok {
        bail!(
            "deploy: health check failed after {} retries",
            options.health_check_retries
        );
    }

    // New VM confirmed healthy — keep it even if later steps fail
    cleanup.disarm();

    // Switch traffic
    println!("deploy: switching traffic for {app}");

    let runner = SshRunner::new(worker.clone());
    if let Some(domain) = options.domain.as_ref() {
        let proxy_target = format!("{}:{}", options.health_check_host, old_port);
        println!("deploy: updating reverse proxy {domain} → {proxy_target}");
        if let Err(e) =
            runner.update_caddy_config(app, domain, &options.health_check_host, old_port)
        {
            println!("deploy: failed to update Caddy config: {e:#}");
        } else if let Err(e) = runner.reload_caddy() {
            println!("deploy: failed to reload Caddy: {e:#}");
        } else {
            println!("deploy: Caddy reloaded");
        }
    } else {
        println!("deploy: update your reverse proxy to route traffic for {app} to the new VM");
        println!("deploy: reload your proxy (e.g. `systemctl reload caddy`)");
    }

    // Drain old VM
    if has_old_vm {
        println!(
            "deploy: draining old VM for {} seconds...",
            options.drain_seconds
        );
        thread::sleep(Duration::from_secs(options.drain_seconds as u64));

        if let Some(ref old_state) = old_state
            && let (Some(pid), Some(remote_runtime_dir)) =
                (old_state.pid, old_state.remote_runtime_dir.as_ref())
            && let Some(ref old_worker_name) = old_state.worker
            && let Ok((_, old_worker)) = config.worker(Some(old_worker_name))
        {
            println!("deploy: stopping old VM for {app} (pid: {pid})");
            let old_runner = SshRunner::new(old_worker.clone());
            old_runner
                .stop_firecracker(pid, remote_runtime_dir)
                .context("failed to stop old VM; new VM is still healthy and running")?;
        }
        old_layout.remove()?;

        // Delete old TAP device (the original one, not deploy-*)
        let old_runner = SshRunner::new(worker.clone());
        let _ = old_runner.delete_tap_interface(&options.tap);
    }

    // Rename remote runtime: {app}-deploy → {app}
    // Must happen AFTER old VM is stopped so the destination directory is free.
    let deploy_remote_dir = ctx.remote_runtime_dir.clone();
    let remote_name_pattern = format!("/{deploy_app}");
    let primary_remote_dir = deploy_remote_dir
        .to_str()
        .map(|s| PathBuf::from(s.replace(&remote_name_pattern, &format!("/{app}"))))
        .and_then(|new_dir| {
            let runner = SshRunner::new(worker.clone());
            match runner.rename_runtime_dir(&deploy_remote_dir, &new_dir) {
                Ok(_) => {
                    println!(
                        "deploy: renamed remote runtime {} → {}",
                        deploy_remote_dir.display(),
                        new_dir.display()
                    );
                    Some(new_dir)
                }
                Err(e) => {
                    println!(
                        "deploy: could not rename remote runtime (keeping {}): {e:#}",
                        deploy_remote_dir.display()
                    );
                    None
                }
            }
        })
        .unwrap_or_else(|| deploy_remote_dir.clone());

    // Swap runtime state: {app}-deploy → {app}
    let deploy_state = RuntimeState::load(&deploy_layout.state_file_path())?;
    let new_layout = RuntimeLayout::from_runtime_dir(&config.runtime_dir, app);
    old_layout.remove()?;
    new_layout.create_dirs()?;
    let primary_api_socket = primary_remote_dir.join("firecracker.sock");
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
        .with_status_message("deployed")
        .save(&new_layout.state_file_path())?;
    deploy_layout.remove()?;

    // Commit registry
    let old_current = registry
        .apps
        .get(app)
        .and_then(|entry| entry.current_image.clone());
    registry.apps.entry(app.to_string()).and_modify(|entry| {
        entry.previous_image = old_current;
        entry.current_image = Some(options.rootfs.clone());
        entry.status = AppStatus::Running;
    });
    registry.save(&config.registry_file)?;

    println!("deploy: {app} deployed successfully");

    Ok(())
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
        match fs::remove_file(path) {
            Ok(()) => {
                println!("unlock: removed {}", path.display());
                removed = true;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                println!("unlock: failed to remove {}: {e}", path.display());
            }
        }
    }

    if !removed {
        println!("unlock: no lock files found for {app}");
    }

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
        Some(pid) => println!("stop: stopped {app} pid {pid}"),
        None => println!("stop: stopped {app}"),
    }

    if let Some(ref worker) = worker_for_proxy {
        let runner = SshRunner::new(worker.clone());
        let _ = runner.remove_caddy_config(app);
        let _ = runner.reload_caddy();
    }

    Ok(())
}

pub fn logs(app: &str) -> Result<()> {
    let config = load_config()?;
    let layout = RuntimeLayout::from_runtime_dir(&config.runtime_dir, app);

    let state = match RuntimeState::load(&layout.state_file_path()) {
        Ok(state) => state,
        Err(error) if is_not_found_error(&error) => {
            println!("logs: {app} is not running");
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
            println!("logs: no logs found for {app}");
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

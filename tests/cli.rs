use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn init_creates_project_local_state() {
    let project = temp_project_dir("init");

    let output = v_command(&project).arg("init").output().expect("run init");

    assert!(output.status.success());
    assert!(project.join(".config/v/config.toml").is_file());
    assert!(project.join(".local/state/v/registry.toml").is_file());
    assert!(project.join(".local/share/v/images").is_dir());
    assert!(project.join(".local/share/v/volumes").is_dir());
    assert!(project.join("runtime/v").is_dir());
    assert!(project.join("runtime/v/locks").is_dir());

    fs::remove_dir_all(project).expect("remove temp project");
}

#[test]
fn ps_reports_empty_registry() {
    let project = initialized_project("ps-empty");

    let output = v_command(&project).arg("ps").output().expect("run ps");

    assert!(output.status.success());
    assert_eq!(stdout(&output), "no apps\n");

    fs::remove_dir_all(project).expect("remove temp project");
}

#[test]
fn ps_reports_registered_apps() {
    let project = initialized_project("ps-apps");
    fs::write(
        project.join(".local/state/v/registry.toml"),
        r#"[apps.web]
current_image = "/images/web-v2.ext4"
previous_image = "/images/web-v1.ext4"
volume_path = "/volumes/web"
port = 8080
status = "running"
"#,
    )
    .expect("write registry");

    let output = v_command(&project).arg("ps").output().expect("run ps");

    assert!(output.status.success());
    assert_eq!(stdout(&output), "web\tRunning\t8080\n");

    fs::remove_dir_all(project).expect("remove temp project");
}

#[test]
fn run_prepares_runtime_state_and_boot_requests() {
    let project = initialized_project("run");
    add_worker_config(&project);

    let output = v_command(&project)
        .args([
            "run",
            "web",
            "--kernel",
            "/kernels/vmlinux",
            "--rootfs",
            "/images/web.ext4",
            "--worker",
            "vps-prod",
            "--tap",
            "tap-web",
            "--vcpu",
            "2",
            "--memory-mib",
            "512",
            "--dry-run",
        ])
        .output()
        .expect("run app");

    assert!(output.status.success(), "{output:?}");
    let stdout = stdout(&output);
    assert!(stdout.contains("runtime:"));
    assert!(stdout.contains("worker: vps-prod (deploy@203.0.113.10)"));
    assert!(stdout.contains(
        "remote runtime: ${XDG_RUNTIME_DIR:-${XDG_STATE_HOME:-$HOME/.local/state}/v/runtime}/v/web"
    ));
    assert!(stdout.contains(
        "api socket: ${XDG_RUNTIME_DIR:-${XDG_STATE_HOME:-$HOME/.local/state}/v/runtime}/v/web/firecracker.sock"
    ));
    assert!(stdout.contains("api socket:"));
    assert!(stdout.contains("PUT /machine-config"));
    assert!(stdout.contains("PUT /boot-source"));
    assert!(stdout.contains("PUT /drives/rootfs"));
    assert!(stdout.contains("PUT /network-interfaces/eth0"));
    assert!(stdout.contains("PUT /actions"));

    let state =
        fs::read_to_string(project.join("runtime/v/web/state.toml")).expect("read runtime state");
    assert!(state.contains("app = \"web\""));
    assert!(state.contains(
        "api_socket = \"${XDG_RUNTIME_DIR:-${XDG_STATE_HOME:-$HOME/.local/state}/v/runtime}/v/web/firecracker.sock\""
    ));
    assert!(state.contains("status = \"starting\""));
    assert!(project.join("runtime/v/web/logs").is_dir());

    fs::remove_dir_all(project).expect("remove temp project");
}

#[test]
fn run_with_fake_ssh_boots_remote_firecracker() {
    let project = initialized_project("run-remote-boot");
    add_worker_config(&project);
    let local_kernel = project.join("vmlinux");
    let local_rootfs = project.join("rootfs.ext4");
    fs::write(&local_kernel, "kernel").expect("write local kernel");
    fs::write(&local_rootfs, "rootfs").expect("write local rootfs");
    let fake_ssh = fake_ssh_bin(&project);
    let fake_ssh_log = project.join("fake-ssh.log");

    let output = v_command(&project)
        .env("V_SSH_BIN", &fake_ssh)
        .env("V_FAKE_SSH_LOG", &fake_ssh_log)
        .args(["run", "web", "--worker", "vps-prod", "--kernel"])
        .arg(&local_kernel)
        .args(["--rootfs"])
        .arg(&local_rootfs)
        .args(["--tap", "tap0"])
        .output()
        .expect("run app");

    assert!(output.status.success(), "{output:?}");
    let stdout = stdout(&output);
    assert!(stdout.contains("worker: vps-prod (deploy@203.0.113.10)"));
    assert!(stdout.contains("remote runtime: /tmp/v-fake-runtime/web"));
    assert!(stdout.contains("api socket: /tmp/v-fake-runtime/web/firecracker.sock"));
    assert!(stdout.contains("kernel: /tmp/v-fake-images/web-kernel-vmlinux"));
    assert!(stdout.contains("rootfs: /tmp/v-fake-images/web-rootfs-rootfs.ext4"));
    assert!(stdout.contains("PUT /machine-config"));
    assert!(stdout.contains("PUT /boot-source"));
    assert!(stdout.contains("PUT /drives/rootfs"));
    assert!(stdout.contains("PUT /network-interfaces/eth0"));
    assert!(stdout.contains("PUT /actions"));
    assert!(stdout.contains("pid: 4242"));

    let ssh_log = fs::read_to_string(fake_ssh_log).expect("read fake ssh log");
    assert!(ssh_log.contains("uname -s"));
    assert!(ssh_log.contains("/dev/kvm"));
    assert!(ssh_log.contains("XDG_RUNTIME_DIR"));
    assert!(ssh_log.contains("log_dir=\"$runtime_dir/logs\""));
    assert!(ssh_log.contains("mkdir -p \"$data_root/images\" \"$data_root/volumes\""));
    assert!(ssh_log.contains("web-kernel-vmlinux"));
    assert!(ssh_log.contains("web-rootfs-rootfs.ext4"));
    assert!(ssh_log.contains("ip tuntap add dev \"$tap\" mode tap"));
    assert!(ssh_log.contains("ip link set \"$tap\" up"));
    assert!(ssh_log.contains("/usr/local/bin/firecracker"));
    assert!(ssh_log.contains("--api-sock"));
    assert!(ssh_log.contains("curl"));
    assert!(ssh_log.contains("/machine-config"));
    assert!(ssh_log.contains("/boot-source"));
    assert!(ssh_log.contains("/drives/rootfs"));
    assert!(ssh_log.contains("/network-interfaces/eth0"));
    assert!(ssh_log.contains("/actions"));

    let state =
        fs::read_to_string(project.join("runtime/v/web/state.toml")).expect("read runtime state");
    assert!(state.contains("app = \"web\""));
    assert!(state.contains("api_socket = \"/tmp/v-fake-runtime/web/firecracker.sock\""));
    assert!(state.contains("worker = \"vps-prod\""));
    assert!(state.contains("remote_runtime_dir = \"/tmp/v-fake-runtime/web\""));
    assert!(state.contains("status = \"running\""));
    assert!(state.contains("pid = 4242"));

    fs::remove_dir_all(project).expect("remove temp project");
}

#[test]
fn check_validates_worker_without_remote_files() {
    let project = initialized_project("check");
    add_worker_config(&project);
    let fake_ssh = fake_ssh_bin(&project);
    let fake_ssh_log = project.join("fake-ssh.log");

    let output = v_command(&project)
        .env("V_SSH_BIN", &fake_ssh)
        .env("V_FAKE_SSH_LOG", &fake_ssh_log)
        .args([
            "check",
            "web",
            "--worker",
            "vps-prod",
            "--skip-kernel",
            "--skip-rootfs",
        ])
        .output()
        .expect("check worker");

    assert!(output.status.success(), "{output:?}");
    let stdout = stdout(&output);
    assert!(stdout.contains("worker: vps-prod (deploy@203.0.113.10)"));
    assert!(stdout.contains("remote runtime:"));
    assert!(stdout.contains("api socket:"));
    assert!(stdout.contains("tap: tap0"));
    assert!(stdout.contains("ok"));
    let ssh_log = fs::read_to_string(fake_ssh_log).expect("read fake ssh log");
    assert!(ssh_log.contains("uname -s"));
    assert!(ssh_log.contains("tap='\"'\"'tap0'\"'\"'"));
    assert!(ssh_log.contains("ip link show dev \"$tap\""));
    assert!(!ssh_log.contains("name='\"'\"'kernel'\"'\"'"));
    assert!(!ssh_log.contains("name='\"'\"'rootfs'\"'\"'"));

    fs::remove_dir_all(project).expect("remove temp project");
}

#[test]
fn check_validates_all_remote_prerequisites_by_default() {
    let project = initialized_project("check-files");
    add_worker_config(&project);
    let fake_ssh = fake_ssh_bin(&project);
    let fake_ssh_log = project.join("fake-ssh.log");

    let output = v_command(&project)
        .env("V_SSH_BIN", &fake_ssh)
        .env("V_FAKE_SSH_LOG", &fake_ssh_log)
        .args([
            "check",
            "web",
            "--worker",
            "vps-prod",
            "--kernel",
            "/kernels/vmlinux",
            "--rootfs",
            "$XDG_DATA_HOME/v/images/web.ext4",
            "--tap",
            "tap-web",
        ])
        .output()
        .expect("check worker files");

    assert!(output.status.success(), "{output:?}");
    let stdout = stdout(&output);
    assert!(stdout.contains("kernel:"));
    assert!(stdout.contains("rootfs:"));
    assert!(stdout.contains("tap: tap-web"));
    let ssh_log = fs::read_to_string(fake_ssh_log).expect("read fake ssh log");
    assert!(ssh_log.contains("name='\"'\"'kernel'\"'\"'"));
    assert!(ssh_log.contains("name='\"'\"'rootfs'\"'\"'"));
    assert!(ssh_log.contains("tap='\"'\"'tap-web'\"'\"'"));

    fs::remove_dir_all(project).expect("remove temp project");
}

#[test]
fn check_requires_kernel_and_rootfs_unless_skipped() {
    let project = initialized_project("check-requires-files");
    add_worker_config(&project);
    let fake_ssh = fake_ssh_bin(&project);
    let fake_ssh_log = project.join("fake-ssh.log");

    let output = v_command(&project)
        .env("V_SSH_BIN", &fake_ssh)
        .env("V_FAKE_SSH_LOG", &fake_ssh_log)
        .args(["check", "web", "--worker", "vps-prod"])
        .output()
        .expect("check worker");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr is utf-8");
    assert!(stderr.contains("kernel path is required unless --skip-kernel is passed"));

    fs::remove_dir_all(project).expect("remove temp project");
}

#[test]
fn run_rejects_invalid_machine_config() {
    let project = initialized_project("run-invalid");

    let output = v_command(&project)
        .args([
            "run",
            "web",
            "--kernel",
            "/kernels/vmlinux",
            "--rootfs",
            "/images/web.ext4",
            "--vcpu",
            "0",
        ])
        .output()
        .expect("run app");

    assert!(!output.status.success());

    fs::remove_dir_all(project).expect("remove temp project");
}

#[test]
fn stop_uses_fake_ssh_for_remote_kill_and_cleanup() {
    let project = initialized_project("stop-remote");
    add_worker_config(&project);
    let fake_ssh = fake_ssh_bin(&project);
    let fake_ssh_log = project.join("fake-ssh.log");
    let runtime_dir = project.join("runtime/v/web");
    fs::create_dir_all(runtime_dir.join("logs")).expect("create local runtime");
    fs::write(
        runtime_dir.join("state.toml"),
        r#"app = "web"
pid = 4242
api_socket = "/tmp/v-fake-runtime/web/firecracker.sock"
worker = "vps-prod"
remote_runtime_dir = "/tmp/v-fake-runtime/web"
status = "running"
status_message = "booted"
"#,
    )
    .expect("write remote runtime state");

    let stop = v_command(&project)
        .env("V_SSH_BIN", &fake_ssh)
        .env("V_FAKE_SSH_LOG", &fake_ssh_log)
        .args(["stop", "web"])
        .output()
        .expect("stop app");

    assert!(stop.status.success(), "{stop:?}");
    assert_eq!(stdout(&stop), "stop: stopped web pid 4242\n");
    assert!(!runtime_dir.exists());

    let ssh_log = fs::read_to_string(fake_ssh_log).expect("read fake ssh log");
    assert!(ssh_log.contains("kill"));
    assert!(ssh_log.contains("4242"));
    assert!(ssh_log.contains("/tmp/v-fake-runtime/web"));
    assert!(ssh_log.contains("rm -rf"));

    fs::remove_dir_all(project).expect("remove temp project");
}

#[test]
fn stub_commands_load_config_and_take_locks() {
    let project = initialized_project("deploy");
    add_worker_config(&project);

    let output = v_command(&project)
        .args([
            "deploy",
            "web",
            "--kernel",
            "/kernels/vmlinux",
            "--rootfs",
            "/images/web-v2.ext4",
            "--dry-run",
        ])
        .output()
        .expect("run deploy");

    assert!(output.status.success(), "deploy failed: {:?}", output);
    assert!(stdout(&output).contains("web"));
    fs::remove_dir_all(project).expect("remove temp project");

    for command in ["rollback", "stop", "logs"] {
        let project = initialized_project(command);

        let output = v_command(&project)
            .args([command, "web"])
            .output()
            .expect("run command");

        assert!(output.status.success(), "{command} failed: {:?}", output);
        assert!(stdout(&output).contains("web"));

        fs::remove_dir_all(project).expect("remove temp project");
    }
}

#[test]
fn logs_reports_when_app_not_running() {
    let project = initialized_project("logs-no-state");

    let output = v_command(&project)
        .args(["logs", "web"])
        .output()
        .expect("run logs");

    assert!(output.status.success(), "{output:?}");
    assert_eq!(stdout(&output), "logs: web is not running\n");

    fs::remove_dir_all(project).expect("remove temp project");
}

#[test]
fn logs_streams_remote_logs_over_ssh() {
    let project = initialized_project("logs-remote");
    add_worker_config(&project);
    let fake_ssh = fake_ssh_bin(&project);
    let fake_ssh_log = project.join("fake-ssh.log");
    let runtime_dir = project.join("runtime/v/web");
    fs::create_dir_all(runtime_dir.join("logs")).expect("create local runtime");
    fs::write(
        runtime_dir.join("state.toml"),
        r#"app = "web"
pid = 4242
api_socket = "/tmp/v-fake-runtime/web/firecracker.sock"
worker = "vps-prod"
remote_runtime_dir = "/tmp/v-fake-runtime/web"
status = "running"
status_message = "booted"
"#,
    )
    .expect("write runtime state");

    let output = v_command(&project)
        .env("V_SSH_BIN", &fake_ssh)
        .env("V_FAKE_SSH_LOG", &fake_ssh_log)
        .args(["logs", "web"])
        .output()
        .expect("run logs");

    assert!(output.status.success(), "{output:?}");

    let ssh_log = fs::read_to_string(fake_ssh_log).expect("read fake ssh log");
    assert!(ssh_log.contains("/tmp/v-fake-runtime/web/logs"));
    assert!(ssh_log.contains("cat"));

    fs::remove_dir_all(project).expect("remove temp project");
}

#[test]
fn ps_shows_runtime_state_for_apps() {
    let project = initialized_project("ps-runtime");
    add_worker_config(&project);
    let runtime_dir = project.join("runtime/v/web");
    fs::create_dir_all(runtime_dir.join("logs")).expect("create local runtime");
    fs::write(
        runtime_dir.join("state.toml"),
        r#"app = "web"
pid = 4242
api_socket = "/tmp/v-fake-runtime/web/firecracker.sock"
worker = "vps-prod"
remote_runtime_dir = "/tmp/v-fake-runtime/web"
status = "running"
status_message = "booted"
"#,
    )
    .expect("write runtime state");

    let current_pid = std::process::id();
    fs::create_dir_all(project.join("runtime/v/api/logs")).expect("create api runtime");
    fs::write(
        project.join("runtime/v/api/state.toml"),
        format!(
            r#"app = "api"
pid = {current_pid}
api_socket = "/tmp/v-fake-runtime/api/firecracker.sock"
status = "starting"
"#,
        ),
    )
    .expect("write api runtime state");

    let output = v_command(&project).arg("ps").output().expect("run ps");

    assert!(output.status.success(), "{output:?}");
    let stdout = stdout(&output);
    assert!(stdout.contains("api\tStarting\t-\t"));
    assert!(stdout.contains("web\tRunning\tvps-prod\t4242\t/tmp/v-fake-runtime/web"));

    fs::remove_dir_all(project).expect("remove temp project");
}

#[test]
fn ps_cleans_up_stale_runtime_state() {
    let project = initialized_project("ps-stale");
    let runtime_dir = project.join("runtime/v/stale-app");
    fs::create_dir_all(runtime_dir.join("logs")).expect("create runtime");
    fs::write(
        runtime_dir.join("state.toml"),
        r#"app = "stale-app"
pid = 99999
api_socket = "/tmp/socket"
status = "running"
"#,
    )
    .expect("write stale state");

    let output = v_command(&project).arg("ps").output().expect("run ps");

    assert!(output.status.success(), "{output:?}");
    let stdout = stdout(&output);
    assert!(!stdout.contains("stale-app"));
    assert!(stdout.contains("cleaned up"));
    assert!(!runtime_dir.exists());

    fs::remove_dir_all(project).expect("remove temp project");
}

#[test]
fn ps_shows_both_runtime_and_registry_apps() {
    let project = initialized_project("ps-mixed");
    add_worker_config(&project);

    fs::write(
        project.join(".local/state/v/registry.toml"),
        r#"[apps.registry-only]
current_image = "/images/reg.ext4"
volume_path = "/volumes/reg"
port = 8080
status = "running"
"#,
    )
    .expect("write registry");

    let current_pid = std::process::id();
    let runtime_dir = project.join("runtime/v/runtime-app");
    fs::create_dir_all(runtime_dir.join("logs")).expect("create runtime");
    fs::write(
        runtime_dir.join("state.toml"),
        format!(
            r#"app = "runtime-app"
pid = {current_pid}
api_socket = "/tmp/sock"
status = "starting"
"#,
        ),
    )
    .expect("write runtime state");

    let output = v_command(&project).arg("ps").output().expect("run ps");

    assert!(output.status.success(), "{output:?}");
    let stdout = stdout(&output);
    assert!(stdout.contains("runtime-app\tStarting"));
    assert!(stdout.contains("registry-only\tRunning\t8080"));

    fs::remove_dir_all(project).expect("remove temp project");
}

#[test]
fn missing_config_fails() {
    let project = temp_project_dir("missing-config");

    let output = v_command(&project).arg("ps").output().expect("run ps");

    assert!(!output.status.success());

    fs::remove_dir_all(project).expect("remove temp project");
}

fn initialized_project(name: &str) -> PathBuf {
    initialized_project_at(&std::env::temp_dir(), name)
}

fn initialized_project_at(parent: &Path, name: &str) -> PathBuf {
    let project = temp_project_dir_at(parent, name);
    let output = v_command(&project).arg("init").output().expect("run init");

    assert!(output.status.success());

    project
}

fn add_worker_config(project: &Path) {
    let config_path = project.join(".config/v/config.toml");
    let mut config = fs::read_to_string(&config_path).expect("read config");
    config.push_str(
        r#"
default_worker = "vps-prod"

[workers.vps-prod]
host = "203.0.113.10"
user = "deploy"
firecracker_bin = "/usr/local/bin/firecracker"
"#,
    );
    fs::write(config_path, config).expect("write worker config");
}

fn v_command(project: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_v"));
    command.current_dir(project);
    command.env("HOME", project);
    command.env("XDG_CONFIG_HOME", project.join(".config"));
    command.env("XDG_DATA_HOME", project.join(".local/share"));
    command.env("XDG_STATE_HOME", project.join(".local/state"));
    command.env("XDG_RUNTIME_DIR", project.join("runtime"));
    command
}

fn temp_project_dir(name: &str) -> PathBuf {
    temp_project_dir_at(&std::env::temp_dir(), name)
}

fn temp_project_dir_at(parent: &Path, name: &str) -> PathBuf {
    let path = parent.join(format!(
        "v-cli-{name}-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    fs::create_dir_all(&path).expect("create temp project");
    path
}

fn stdout(output: &std::process::Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout is utf-8")
}

fn fake_ssh_bin(project: &Path) -> PathBuf {
    let path = project.join("fake-ssh");
    fs::write(
        &path,
        r#"#!/bin/sh
set -eu
log="${V_FAKE_SSH_LOG:?}"
remote_runtime="${V_FAKE_REMOTE_RUNTIME:-/tmp/v-fake-runtime/web}"
pid="${V_FAKE_FIRECRACKER_PID:-4242}"

printf '%s\n' "$*" >> "$log"

case "$*" in
  *"destination="*"web-kernel-vmlinux"*)
    cat >/dev/null
    printf '%s\n' "/tmp/v-fake-images/web-kernel-vmlinux"
    ;;
  *"destination="*"web-rootfs-rootfs.ext4"*)
    cat >/dev/null
    printf '%s\n' "/tmp/v-fake-images/web-rootfs-rootfs.ext4"
    ;;
  *"--api-sock"* | *"/machine-config"* | *"/actions"*)
    printf '%s\n' "$pid"
    ;;
  *"runtime_dir="*"mkdir -p"*)
    printf '%s\n' "$remote_runtime"
    ;;
  *"name='\"'\"'kernel'\"'\"'"*)
    printf '%s\n' "/kernels/vmlinux"
    ;;
  *"name='\"'\"'rootfs'\"'\"'"*)
    printf '%s\n' "/images/web.ext4"
    ;;
  *"tap='\"'\"'tap0'\"'\"'"*)
    printf '%s\n' "tap0"
    ;;
  *"tap='\"'\"'tap-web'\"'\"'"*)
    printf '%s\n' "tap-web"
    ;;
  *)
    :
    ;;
esac
"#,
    )
    .expect("write fake ssh");

    let mut permissions = fs::metadata(&path)
        .expect("read fake ssh metadata")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&path, permissions).expect("mark fake ssh executable");

    path
}

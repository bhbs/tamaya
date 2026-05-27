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
    assert!(stdout(&output).contains("no apps"));

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
fn check_validates_worker_without_remote_files() {
    let project = initialized_project("check");
    add_worker_config(&project);
    let fake_ssh = fake_ssh_bin(&project);
    let fake_ssh_log = project.join("fake-ssh.log");

    let output = v_command(&project)
        .env("V_SSH_BIN", &fake_ssh)
        .env("V_FAKE_SSH_LOG", &fake_ssh_log)
        .args(["check", "web", "--worker", "vps-prod"])
        .output()
        .expect("check worker");

    assert!(output.status.success(), "{output:?}");
    let stdout = stdout(&output);
    assert!(stdout.contains("worker: vps-prod (deploy@203.0.113.10)"));
    assert!(stdout.contains("remote runtime:"));
    assert!(stdout.contains("api socket:"));
    assert!(stdout.contains("ok"));
    let ssh_log = fs::read_to_string(fake_ssh_log).expect("read fake ssh log");
    assert!(ssh_log.contains("uname -s"));
    assert!(!ssh_log.contains("ip link show dev \"$tap\""));
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
fn check_does_not_require_boot_files() {
    let project = initialized_project("check-without-files");
    add_worker_config(&project);
    let fake_ssh = fake_ssh_bin(&project);
    let fake_ssh_log = project.join("fake-ssh.log");

    let output = v_command(&project)
        .env("V_SSH_BIN", &fake_ssh)
        .env("V_FAKE_SSH_LOG", &fake_ssh_log)
        .args(["check", "web", "--worker", "vps-prod"])
        .output()
        .expect("check worker");

    assert!(output.status.success(), "{output:?}");
    let stdout = stdout(&output);
    assert!(stdout.contains("worker: vps-prod (deploy@203.0.113.10)"));
    assert!(stdout.contains("ok"));
    let ssh_log = fs::read_to_string(fake_ssh_log).expect("read fake ssh log");
    assert!(!ssh_log.contains("name='\"'\"'kernel'\"'\"'"));
    assert!(!ssh_log.contains("name='\"'\"'rootfs'\"'\"'"));

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
    assert!(stdout(&stop).contains("stop: stopped web pid 4242"));
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
    assert!(stdout(&output).contains("logs: web is not running"));

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
fn deploy_dry_run_shows_proxy_update() {
    let project = initialized_project("deploy-proxy-dry");
    add_worker_config_with_caddy(&project);

    let output = v_command(&project)
        .args([
            "deploy",
            "web",
            "--kernel",
            "/kernels/vmlinux",
            "--rootfs",
            "/images/web-v2.ext4",
            "--domain",
            "web.example.com",
            "--dry-run",
        ])
        .output()
        .expect("run deploy");

    assert!(output.status.success(), "{output:?}");
    let stdout = stdout(&output);
    assert!(stdout.contains("would update reverse proxy: web.example.com → 10.0.0.2:8080"));
    assert!(stdout.contains("would reload Caddy"));

    fs::remove_dir_all(project).expect("remove temp project");
}

#[test]
fn deploy_dry_run_without_domain_shows_manual_proxy() {
    let project = initialized_project("deploy-nodomain-dry");
    add_worker_config_with_caddy(&project);

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

    assert!(output.status.success(), "{output:?}");
    let stdout = stdout(&output);
    assert!(
        stdout.contains("(no --domain set; proxy routing is manual)"),
        "got: {stdout}"
    );

    fs::remove_dir_all(project).expect("remove temp project");
}

#[test]
fn build_dry_run_reports_planned_app_paths_without_external_tools() {
    let project = initialized_project("build-dry");
    let context = project.join("app-src");
    let dockerfile = context.join("Dockerfile");
    fs::create_dir_all(&context).expect("create app context");
    fs::write(&dockerfile, "FROM debian:bookworm-slim\n").expect("write Dockerfile");

    let fake_bin = project.join("fake-bin");
    fs::create_dir_all(&fake_bin).expect("create fake bin");
    let external_tool_log = project.join("external-tools.log");
    for tool in ["docker", "mkfs.ext4", "mkfs"] {
        let shim = fake_bin.join(tool);
        fs::write(
            &shim,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"{tool} $*\" >> \"{}\"\nexit 99\n",
                external_tool_log.display()
            ),
        )
        .expect("write external tool shim");
        let mut permissions = fs::metadata(&shim)
            .expect("read shim metadata")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&shim, permissions).expect("chmod shim");
    }

    let output = v_command(&project)
        .env("PATH", &fake_bin)
        .args(["build", "web", "--context"])
        .arg(&context)
        .args(["--dockerfile"])
        .arg(&dockerfile)
        .arg("--dry-run")
        .output()
        .expect("run build dry-run");

    assert!(output.status.success(), "{output:?}");
    assert!(
        !external_tool_log.exists(),
        "dry-run should not call docker or mkfs"
    );

    let stdout = stdout(&output);
    for expected in [
        ".local/share/v/apps/web",
        ".local/share/v/apps/web/artifact.tar",
        ".local/share/v/apps/web/config.json",
        ".local/share/v/apps/web/metadata.json",
    ] {
        assert!(stdout.contains(expected), "missing {expected} in {stdout}");
    }

    fs::remove_dir_all(project).expect("remove temp project");
}

#[test]
fn deploy_recovers_stale_deploying_registry_with_running_runtime() {
    let project = initialized_project("deploy-stale-registry");
    fs::write(
        project.join(".local/state/v/registry.toml"),
        r#"[apps.web]
current_image = "/images/web-v1.ext4"
volume_path = "/volumes/web"
port = 8080
status = "deploying"
"#,
    )
    .expect("write registry");
    fs::create_dir_all(project.join("runtime/v/web")).expect("create runtime");
    fs::write(
        project.join("runtime/v/web/state.toml"),
        r#"app = "web"
pid = 4242
api_socket = "/run/user/0/v/web/firecracker.sock"
worker = "fire"
remote_runtime_dir = "/run/user/0/v/web"
status = "running"
"#,
    )
    .expect("write runtime state");

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

    assert!(output.status.success(), "{output:?}");
    let stdout = stdout(&output);
    assert!(stdout.contains("previous deploy was interrupted; resetting status and retrying"));
    assert!(stdout.contains("deploy: dry-run for web"));

    let registry =
        fs::read_to_string(project.join(".local/state/v/registry.toml")).expect("read registry");
    assert!(registry.contains("status = \"running\""));

    fs::remove_dir_all(project).expect("remove temp project");
}

#[test]
fn deploy_blocks_when_deploy_runtime_state_exists() {
    let project = initialized_project("deploy-active");
    fs::write(
        project.join(".local/state/v/registry.toml"),
        r#"[apps.web]
current_image = "/images/web-v1.ext4"
volume_path = "/volumes/web"
port = 8080
status = "deploying"
"#,
    )
    .expect("write registry");
    fs::create_dir_all(project.join("runtime/v/web-deploy")).expect("create deploy runtime");
    fs::write(
        project.join("runtime/v/web-deploy/state.toml"),
        r#"app = "web-deploy"
pid = 4242
api_socket = "/run/user/0/v/web-deploy/firecracker.sock"
worker = "vps-prod"
remote_runtime_dir = "/run/user/0/v/web-deploy"
tap = "t-deadcafe0000"
status = "running"
"#,
    )
    .expect("write deploy runtime state");

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

    assert!(!output.status.success(), "{output:?}");
    let stderr = String::from_utf8(output.stderr).expect("stderr is utf-8");
    assert!(stderr.contains("web: deploy is already in progress"));

    fs::remove_dir_all(project).expect("remove temp project");
}

#[test]
fn deploy_cleans_up_stale_deploy_runtime_and_proceeds() {
    let project = initialized_project("deploy-stale-deploy");
    fs::write(
        project.join(".local/state/v/registry.toml"),
        r#"[apps.web]
current_image = "/images/web-v1.ext4"
volume_path = "/volumes/web"
port = 8080
status = "deploying"
"#,
    )
    .expect("write registry");
    fs::create_dir_all(project.join("runtime/v/web")).expect("create runtime");
    fs::write(
        project.join("runtime/v/web/state.toml"),
        r#"app = "web"
pid = 4242
api_socket = "/run/user/0/v/web/firecracker.sock"
worker = "vps-prod"
remote_runtime_dir = "/run/user/0/v/web"
status = "running"
"#,
    )
    .expect("write runtime state");
    fs::create_dir_all(project.join("runtime/v/web-deploy")).expect("create deploy runtime");
    fs::write(
        project.join("runtime/v/web-deploy/state.toml"),
        r#"app = "web-deploy"
api_socket = "/run/user/0/v/web-deploy/firecracker.sock"
status = "stopped"
"#,
    )
    .expect("write deploy runtime state");

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

    assert!(output.status.success(), "{output:?}");
    let stdout = stdout(&output);
    assert!(stdout.contains("cleaning up stale deploy resources"));
    assert!(stdout.contains("deploy: dry-run for web"));
    assert!(!project.join("runtime/v/web-deploy").exists());

    fs::remove_dir_all(project).expect("remove temp project");
}

#[test]
fn deploy_with_fake_ssh_updates_caddy_and_reloads() {
    let project = initialized_project("deploy-caddy");
    add_worker_config_with_caddy(&project);
    let local_kernel = project.join("vmlinux");
    let local_rootfs = project.join("rootfs.ext4");
    fs::write(&local_kernel, "kernel").expect("write local kernel");
    fs::write(&local_rootfs, "rootfs").expect("write local rootfs");
    let fake_ssh = fake_ssh_bin(&project);
    let fake_ssh_log = project.join("fake-ssh.log");

    let output = v_command(&project)
        .env("V_SSH_BIN", &fake_ssh)
        .env("V_FAKE_SSH_LOG", &fake_ssh_log)
        .env("V_FAKE_REMOTE_RUNTIME", "/tmp/v-fake-runtime/web-deploy")
        .args(["deploy", "web", "--worker", "vps-prod", "--kernel"])
        .arg(&local_kernel)
        .args(["--rootfs"])
        .arg(&local_rootfs)
        .args(["--tap", "tap0"])
        .args(["--domain", "web.example.com"])
        .args(["--skip-health-check"])
        .output()
        .expect("run deploy");

    assert!(output.status.success(), "{output:?}");
    let stdout = stdout(&output);
    assert!(stdout.contains("updating reverse proxy web.example.com → 10.0.0.2:8080"));
    assert!(stdout.contains("Caddy reloaded"));

    let ssh_log = fs::read_to_string(fake_ssh_log).expect("read fake ssh log");
    assert!(ssh_log.contains("sudo mkdir -p \"$config_dir\""));
    assert!(ssh_log.contains("sudo tee \"$config_file\""));
    assert!(ssh_log.contains("web.example.com {"));
    assert!(ssh_log.contains("reverse_proxy 10.0.0.2:8080"));
    assert!(ssh_log.contains("sudo systemctl reload caddy"));

    let state =
        fs::read_to_string(project.join("runtime/v/web/state.toml")).expect("read runtime state");
    assert!(state.contains("app = \"web\""));
    assert!(state.contains("api_socket = \"/tmp/v-fake-runtime/web/firecracker.sock\""));
    assert!(state.contains("remote_runtime_dir = \"/tmp/v-fake-runtime/web\""));
    assert!(!project.join("runtime/v/web-deploy").exists());

    fs::remove_dir_all(project).expect("remove temp project");
}

#[test]
fn deploy_artifact_materializes_rootfs_and_attaches_data_on_worker() {
    let project = initialized_project("deploy-artifact");
    add_worker_config(&project);
    let local_kernel = project.join("vmlinux");
    let local_artifact = project.join("artifact.tar");
    fs::write(&local_kernel, "kernel").expect("write local kernel");
    fs::write(&local_artifact, "artifact").expect("write local artifact");
    let fake_ssh = fake_ssh_bin(&project);
    let fake_ssh_log = project.join("fake-ssh.log");

    let output = v_command(&project)
        .env("V_SSH_BIN", &fake_ssh)
        .env("V_FAKE_SSH_LOG", &fake_ssh_log)
        .env("V_FAKE_REMOTE_RUNTIME", "/tmp/v-fake-runtime/web-deploy")
        .args(["deploy", "web", "--worker", "vps-prod", "--kernel"])
        .arg(&local_kernel)
        .args(["--artifact"])
        .arg(&local_artifact)
        .args(["--skip-health-check"])
        .output()
        .expect("run artifact deploy");

    assert!(output.status.success(), "{output:?}");
    let stdout = stdout(&output);
    assert!(stdout.contains("uploading artifact"));
    assert!(stdout.contains("materializing rootfs.ext4 and data.ext4 on worker"));
    assert!(stdout.contains("worker rootfs /tmp/v-fake-apps/web/rootfs.ext4"));
    assert!(stdout.contains("worker data /tmp/v-fake-apps/web/data.ext4"));

    let ssh_log = fs::read_to_string(fake_ssh_log).expect("read fake ssh log");
    assert!(ssh_log.contains("artifact_dir="));
    assert!(ssh_log.contains("rootfs_size_mib=1024"));
    assert!(ssh_log.contains("/drives/data"));
    assert!(ssh_log.contains("\"drive_id\":\"data\""));

    fs::remove_dir_all(project).expect("remove temp project");
}

#[test]
fn setup_installs_caddy_on_worker() {
    let project = initialized_project("setup-caddy");
    add_worker_config_with_caddy(&project);
    let fake_ssh = fake_ssh_bin(&project);
    let fake_ssh_log = project.join("fake-ssh.log");

    let output = v_command(&project)
        .env("V_SSH_BIN", &fake_ssh)
        .env("V_FAKE_SSH_LOG", &fake_ssh_log)
        .args(["setup", "--worker", "vps-prod"])
        .output()
        .expect("run setup");

    assert!(output.status.success(), "{output:?}");
    let stdout = stdout(&output);
    assert!(stdout.contains("worker prerequisites: installed"));

    let ssh_log = fs::read_to_string(fake_ssh_log).expect("read fake ssh log");
    assert!(ssh_log.contains("firecracker_bin="));
    assert!(ssh_log.contains("caddy_config_dir="));
    assert!(ssh_log.contains("sudo apt-get install"));

    fs::remove_dir_all(project).expect("remove temp project");
}

#[test]
fn cleanup_removes_stale_taps_on_worker() {
    let project = initialized_project("cleanup-taps");
    add_worker_config(&project);
    let fake_ssh = fake_ssh_bin(&project);
    let fake_ssh_log = project.join("fake-ssh.log");

    let output = v_command(&project)
        .env("V_SSH_BIN", &fake_ssh)
        .env("V_FAKE_SSH_LOG", &fake_ssh_log)
        .args(["cleanup", "--worker", "vps-prod", "--stale-taps"])
        .output()
        .expect("run cleanup");

    assert!(output.status.success(), "{output:?}");
    let stdout = stdout(&output);
    assert!(stdout.contains("cleanup: worker vps-prod"));
    assert!(stdout.contains("cleanup: removed TAP t-stale"));
    assert!(stdout.contains("cleanup: removed TAP web-deploy"));
    assert!(stdout.contains("cleanup: removed 2 stale TAP interfaces"));

    let ssh_log = fs::read_to_string(fake_ssh_log).expect("read fake ssh log");
    assert!(ssh_log.contains("ip tuntap show"));
    assert!(ssh_log.contains("ip -brief link show dev \"$tap\""));

    fs::remove_dir_all(project).expect("remove temp project");
}

#[test]
fn cleanup_preserves_deploy_runtime_state_taps() {
    let project = initialized_project("cleanup-deploy-preserve");
    add_worker_config(&project);

    fs::create_dir_all(project.join("runtime/v/web-deploy")).expect("create deploy runtime");
    fs::write(
        project.join("runtime/v/web-deploy/state.toml"),
        r#"app = "web-deploy"
api_socket = "/run/user/0/v/web-deploy/firecracker.sock"
worker = "vps-prod"
tap = "t-deploy-in-progress"
status = "starting"
"#,
    )
    .expect("write deploy runtime state");

    let fake_ssh = fake_ssh_bin(&project);
    let fake_ssh_log = project.join("fake-ssh.log");

    let output = v_command(&project)
        .env("V_SSH_BIN", &fake_ssh)
        .env("V_FAKE_SSH_LOG", &fake_ssh_log)
        .args(["cleanup", "--worker", "vps-prod", "--stale-taps"])
        .output()
        .expect("run cleanup");

    assert!(output.status.success(), "{output:?}");
    let stdout = stdout(&output);
    assert!(stdout.contains("cleanup: worker vps-prod"));
    assert!(!stdout.contains("t-deploy-in-progress"));

    let ssh_log = fs::read_to_string(fake_ssh_log).expect("read fake ssh log");
    assert!(ssh_log.contains("t-deploy-in-progress"));
    assert!(
        ssh_log.contains("ip tuntap show"),
        "SSH log should contain stale TAP cleanup commands:\n{ssh_log}"
    );

    fs::remove_dir_all(project).expect("remove temp project");
}

#[test]
fn check_with_caddy_config_detects_caddy() {
    let project = initialized_project("check-caddy");
    add_worker_config_with_caddy(&project);
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
        .expect("run check");

    assert!(output.status.success(), "{output:?}");
    let stdout = stdout(&output);
    assert!(stdout.contains("caddy: installed and running"));

    let ssh_log = fs::read_to_string(fake_ssh_log).expect("read fake ssh log");
    assert!(ssh_log.contains("command -v caddy"));

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

fn add_worker_config_with_caddy(project: &Path) {
    let config_path = project.join(".config/v/config.toml");
    let mut config = fs::read_to_string(&config_path).expect("read config");
    config.push_str(
        r#"
default_worker = "vps-prod"

[workers.vps-prod]
host = "203.0.113.10"
user = "deploy"
firecracker_bin = "/usr/local/bin/firecracker"
caddy_config_dir = "/etc/caddy/conf.d"
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
  *"firecracker_bin="*"caddy_config_dir="*)
    printf '%s\n' "install_worker_prerequisites"
    ;;
  *"destination="*"-kernel-vmlinux"*)
    cat >/dev/null
    printf '%s\n' "/tmp/v-fake-images/web-kernel-vmlinux"
    ;;
  *"destination="*"-rootfs-rootfs.ext4"*)
    cat >/dev/null
    printf '%s\n' "/tmp/v-fake-images/web-rootfs-rootfs.ext4"
    ;;
  *"artifact_dir="*"artifact.tar"*)
    cat >/dev/null
    printf '%s\n' "/tmp/v-fake-artifacts/web/artifact.tar"
    ;;
  *"rootfs_size_mib="*"mkfs_bin="*)
    printf '%s\n' "/tmp/v-fake-apps/web/rootfs.ext4"
    printf '%s\n' "/tmp/v-fake-apps/web/data.ext4"
    printf '%s\n' "/tmp/v-fake-apps/web/config.json"
    printf '%s\n' "/tmp/v-fake-apps/web/metadata.json"
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
  *"name='\"'\"'data'\"'\"'"*)
    printf '%s\n' "/tmp/v-fake-apps/web/data.ext4"
    ;;
  *"tap='\"'\"'tap0'\"'\"'"*)
    printf '%s\n' "tap0"
    ;;
  *"tap='\"'\"'tap-web'\"'\"'"*)
    printf '%s\n' "tap-web"
    ;;
  *"ip tuntap show"*"ip -brief link show"*)
    printf '%s\n' "t-stale"
    printf '%s\n' "web-deploy"
    ;;
  *"tap='"*)
    printf '%s\n' "tap-fake"
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

use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn init_creates_project_local_state() {
    let project = temp_project_dir("init");

    let output = v_command(&project).arg("init").output().expect("run init");

    assert!(output.status.success());
    assert!(project.join(".v/config.toml").is_file());
    assert!(project.join(".v/registry.toml").is_file());
    assert!(project.join(".v/images").is_dir());
    assert!(project.join(".v/volumes").is_dir());
    assert!(project.join(".v/runtime").is_dir());
    assert!(project.join(".v/locks").is_dir());

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
        project.join(".v/registry.toml"),
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

    let output = v_command(&project)
        .args([
            "run",
            "web",
            "--kernel",
            "/kernels/vmlinux",
            "--rootfs",
            "/images/web.ext4",
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
    assert!(stdout.contains("api socket:"));
    assert!(stdout.contains("PUT /machine-config"));
    assert!(stdout.contains("PUT /boot-source"));
    assert!(stdout.contains("PUT /drives/rootfs"));
    assert!(stdout.contains("PUT /network-interfaces/eth0"));
    assert!(stdout.contains("PUT /actions"));

    let state =
        fs::read_to_string(project.join(".v/runtime/web/state.toml")).expect("read runtime state");
    assert!(state.contains("app = \"web\""));
    assert!(state.contains("api_socket = "));
    assert!(state.contains("status = \"starting\""));
    assert!(project.join(".v/runtime/web/logs").is_dir());

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
#[ignore = "requires Unix domain sockets, which are blocked in the sandboxed test environment"]
fn run_boots_with_fake_firecracker_and_stop_cleans_runtime() {
    let project = initialized_project_at(Path::new("/private/tmp"), "run-boot");
    let fake_firecracker = fake_firecracker_bin(&project);
    let api_socket = project.join(".v/runtime/web/firecracker.sock");
    let api_ready = api_socket.with_extension("sock.ready");
    let api_thread = fake_firecracker_api(api_socket.clone(), api_ready);

    let output = v_command(&project)
        .args([
            "run",
            "web",
            "--kernel",
            "/kernels/vmlinux",
            "--rootfs",
            "/images/web.ext4",
            "--firecracker-bin",
        ])
        .arg(&fake_firecracker)
        .args(["--tap", "tap-web"])
        .output()
        .expect("run app");

    assert!(output.status.success(), "{output:?}");
    api_thread.join().expect("fake Firecracker API thread");
    let run_stdout = stdout(&output);
    assert!(run_stdout.contains("PUT /actions"));
    assert!(run_stdout.contains("pid:"));

    let state =
        fs::read_to_string(project.join(".v/runtime/web/state.toml")).expect("read runtime state");
    assert!(state.contains("status = \"running\""));
    assert!(state.contains("pid = "));

    let stop = v_command(&project)
        .args(["stop", "web"])
        .output()
        .expect("stop app");

    assert!(stop.status.success(), "{stop:?}");
    assert_eq!(stdout(&stop), "stop: stopped web\n");
    assert!(!project.join(".v/runtime/web").exists());

    fs::remove_dir_all(project).expect("remove temp project");
}

#[test]
fn stub_commands_load_config_and_take_locks() {
    for command in ["deploy", "rollback", "stop", "logs"] {
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

fn v_command(project: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_v"));
    command.current_dir(project);
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

fn fake_firecracker_bin(project: &Path) -> PathBuf {
    let path = project.join("fake-firecracker");
    fs::write(
        &path,
        r#"#!/bin/sh
set -eu
sock=""
while [ "$#" -gt 0 ]; do
    if [ "$1" = "--api-sock" ]; then
        shift
        sock="$1"
    fi
    shift || true
done

touch "$sock.ready"
sleep 60
"#,
    )
    .expect("write fake firecracker");

    let mut permissions = fs::metadata(&path)
        .expect("read fake firecracker metadata")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&path, permissions).expect("mark fake firecracker executable");

    path
}

fn fake_firecracker_api(socket_path: PathBuf, ready_path: PathBuf) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        while !ready_path.exists() {
            thread::sleep(Duration::from_millis(10));
        }

        let listener = UnixListener::bind(&socket_path).expect("bind fake Firecracker API");
        for _ in 0..5 {
            let (mut stream, _) = listener.accept().expect("accept Firecracker API request");
            let mut request = Vec::new();
            stream
                .read_to_end(&mut request)
                .expect("read Firecracker API request");
            assert!(request.starts_with(b"PUT "));
            stream
                .write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n")
                .expect("write Firecracker API response");
        }
    })
}

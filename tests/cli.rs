use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
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
    let project = temp_project_dir(name);
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
    let path = std::env::temp_dir().join(format!(
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

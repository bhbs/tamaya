use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn worker_progress_events_are_logged_and_removed_from_errors() {
    let home = initialized("progress-events");
    add_worker(&home);
    let output = tamaya(&home)
        .env("TAMAYA_SSH_BIN", progress_then_failure_ssh(&home))
        .arg("setup")
        .output()
        .unwrap();
    assert!(!output.status.success(), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("→ remote setup stage"));
    assert!(stderr.contains("remote failure"));
    assert!(!stderr.contains("__TAMAYA_PROGRESS__"));
    assert!(stderr.contains("✗ tamaya setup failed"));
}

#[test]
fn logs_preserve_streamed_stdout_and_report_stream_start() {
    let home = initialized("logs-progress");
    add_worker(&home);
    let output = tamaya(&home)
        .env("TAMAYA_SSH_BIN", streaming_logs_ssh(&home))
        .args(["logs", "web"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(String::from_utf8_lossy(&output.stdout), "journal line\n");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("→ streaming logs"));
    assert!(!stderr.contains("__TAMAYA_PROGRESS__"));
}

#[test]
fn missing_project_worker_is_reported() {
    let home = temp_dir("missing-project-worker");
    let output = tamaya(&home).args(["status", "web"]).output().unwrap();
    assert!(!output.status.success(), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stderr).contains("worker is required"));
}

#[test]
fn deploy_dry_run_uses_project_config() {
    let home = initialized("deploy-dry-run");
    let binary = home.join("web");
    fs::write(&binary, "#!/bin/sh\n").unwrap();
    fs::write(
        home.join(".tamaya.toml"),
        "name = \"web\"\nbinary = \"./web\"\nworker = \"prod\"\n",
    )
    .unwrap();
    add_worker(&home);
    let output = tamaya(&home)
        .args([
            "--project-dir",
            home.to_str().unwrap(),
            "deploy",
            "--dry-run",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("deploy web to prod"));
    assert!(stdout.contains("health: /health"));
    assert!(stdout.contains("verify_binary_deps: false"));
}

#[test]
fn deploy_dry_run_accepts_valid_process_config_with_path() {
    let home = initialized("deploy-process-path");
    fs::write(home.join("api"), "binary").unwrap();
    fs::write(
        home.join(".tamaya.toml"),
        r#"name = "api"
worker = "prod"
binary = "./api"
domain = "example.com"
path = "/api/"
"#,
    )
    .unwrap();
    add_worker(&home);
    let output = tamaya(&home)
        .args(["deploy", "--dry-run"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("deploy api to prod"));
    assert!(stdout.contains("domain: example.com"));
    assert!(stdout.contains("path: /api"));
}

#[test]
fn deploy_path_requires_domain() {
    let home = initialized("deploy-path-no-domain");
    fs::write(home.join("api"), "binary").unwrap();
    fs::write(
        home.join(".tamaya.toml"),
        r#"name = "api"
worker = "prod"
binary = "./api"
path = "/api"
"#,
    )
    .unwrap();
    add_worker(&home);
    let output = tamaya(&home)
        .args(["deploy", "--dry-run"])
        .output()
        .unwrap();
    assert!(!output.status.success(), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stderr).contains("path deploys require domain"));
}

#[test]
fn publish_validates_published_config_before_worker_publish() {
    let home = initialized("publish-valid-config");
    fs::create_dir_all(home.join("dist/docs")).unwrap();
    fs::write(home.join("dist/docs/index.html"), "<h1>docs</h1>").unwrap();
    fs::create_dir_all(home.join("dist/docs/.well-known")).unwrap();
    fs::write(home.join("dist/docs/.well-known/security.txt"), "contact").unwrap();
    fs::create_dir_all(home.join("dist/.git")).unwrap();
    fs::write(home.join("dist/.git/config"), "secret").unwrap();
    fs::write(home.join("dist/.env"), "SECRET=1").unwrap();
    fs::write(
        home.join(".tamaya.toml"),
        r#"name = "docs"
worker = "prod"
domain = "http://example.com"
path = "/docs/"
static_root = "./dist"
"#,
    )
    .unwrap();
    add_worker(&home);
    let ssh = fake_ssh(&home);
    let log = home.join("ssh.log");
    let output = tamaya(&home)
        .env("TAMAYA_SSH_BIN", ssh)
        .env("TAMAYA_FAKE_SSH_LOG", &log)
        .arg("publish")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("publishing docs to prod"));
    let script = fs::read_to_string(log).unwrap();
    assert!(script.contains("domain='\"'\"'http://example.com'\"'\"'"));
    assert!(script.contains("path='\"'\"'/docs'\"'\"'"));
    assert!(script.contains("publish_type='\"'\"'static'\"'\"'"));
    assert!(
        script.contains("ensure_route_compatible \"$app\" \"published\" \"$domain\" \"$path\"")
    );
    assert!(script.contains(
        "caddy_write_published_route_snippet \"$app\" \"$metadata_path\" \"$site_dir\" \"$publish_type\""
    ));
    assert!(script.contains("try_files {path} {path}.html {path}/ /404.html"));
}

#[test]
fn publish_rejects_invalid_static_roots_before_ssh() {
    let home = initialized("publish-invalid-roots");
    add_worker(&home);

    fs::create_dir_all(home.join("empty")).unwrap();
    fs::write(
        home.join(".tamaya.toml"),
        r#"name = "docs"
worker = "prod"
domain = "example.com"
static_root = "./empty"
"#,
    )
    .unwrap();
    let output = tamaya(&home).arg("publish").output().unwrap();
    assert!(!output.status.success(), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stderr).contains("static_root must not be empty"));

    fs::create_dir_all(home.join("spa")).unwrap();
    fs::write(home.join("spa/readme.txt"), "missing index").unwrap();
    fs::write(
        home.join(".tamaya.toml"),
        r#"name = "docs"
worker = "prod"
domain = "example.com"
publish_type = "spa"
static_root = "./spa"
"#,
    )
    .unwrap();
    let output = tamaya(&home).arg("publish").output().unwrap();
    assert!(!output.status.success(), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("publish_type spa requires index.html")
    );

    fs::create_dir_all(home.join("prefixed/assets")).unwrap();
    fs::write(home.join("prefixed/assets/app.css"), "body{}").unwrap();
    fs::write(
        home.join(".tamaya.toml"),
        r#"name = "docs"
worker = "prod"
domain = "example.com"
path = "/docs"
static_root = "./prefixed"
"#,
    )
    .unwrap();
    let output = tamaya(&home).arg("publish").output().unwrap();
    assert!(!output.status.success(), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stderr).contains("for path-based publish"));
}

#[test]
fn deploy_verify_binary_deps_flag_is_sent_to_worker() {
    let home = initialized("deploy-verify-deps");
    add_worker(&home);
    let binary = home.join("web");
    fs::write(&binary, "binary").unwrap();
    let ssh = fake_ssh(&home);
    let log = home.join("ssh.log");
    let output = tamaya(&home)
        .env("TAMAYA_SSH_BIN", ssh)
        .env("TAMAYA_FAKE_SSH_LOG", &log)
        .args([
            "deploy",
            "web",
            "--worker",
            "prod",
            "--binary",
            binary.to_str().unwrap(),
            "--verify-binary-deps",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let script = fs::read_to_string(log).unwrap();
    assert!(script.contains("checking binary dependencies"));
}

#[test]
fn domain_maintenance_and_live_use_domain_state_scripts() {
    let home = initialized("domain-maintenance-live");
    add_worker(&home);
    let ssh = fake_ssh(&home);
    let log = home.join("ssh.log");

    let output = tamaya(&home)
        .env("TAMAYA_SSH_BIN", &ssh)
        .env("TAMAYA_FAKE_SSH_LOG", &log)
        .args([
            "maintenance",
            "--domain",
            "http://example.com",
            "--message",
            "Back shortly",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");

    let output = tamaya(&home)
        .env("TAMAYA_SSH_BIN", &ssh)
        .env("TAMAYA_FAKE_SSH_LOG", &log)
        .args(["live", "--domain", "http://example.com"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");

    let script = fs::read_to_string(log).unwrap();
    assert!(script.contains("sudo tee \"$domain_dir/$domain_key_value.maintenance\""));
    assert!(script.contains("sudo rm -f \"$domain_dir/$domain_key_value.maintenance\""));
    assert!(script.contains("rebuild_domain \"$domain\""));
    assert!(script.contains("Tamaya has no known apps for $domain"));
    assert!(script.contains("$domain is not in maintenance"));
}

#[test]
fn deploy_streams_binary_to_worker_and_uses_worker_env_file() {
    let home = initialized("deploy");
    add_worker(&home);
    let binary = home.join("web");
    fs::write(&binary, "binary").unwrap();
    let ssh = fake_ssh(&home);
    let log = home.join("ssh.log");
    let output = tamaya(&home)
        .env("TAMAYA_SSH_BIN", ssh)
        .env("TAMAYA_FAKE_SSH_LOG", &log)
        .args(["deploy", "web", "--worker", "prod", "--binary"])
        .arg(binary)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let script = fs::read_to_string(log).unwrap();
    assert!(script.contains("Environment=PORT=$port"));
    assert!(
        script.contains("reverse_proxy 127.0.0.1:$write_port")
            || script.contains("reverse_proxy 127.0.0.1:$port")
            || script.contains("domain=''")
    );
    assert!(script.contains("/etc/tamaya/apps/web.env"));
}

#[test]
fn deploy_applies_project_health_and_resource_limits() {
    let home = initialized("deploy-resources");
    add_worker(&home);
    fs::write(home.join("web"), "binary").unwrap();
    fs::write(
        home.join(".tamaya.toml"),
        r#"name = "web"
worker = "prod"
binary = "./web"
domain = "web.example.com"
[health_check]
path = "/healthz"
retries = 2
interval_secs = 1
timeout_secs = 1
[memory]
max = "512M"
[cpu]
quota = "50%"
"#,
    )
    .unwrap();
    let ssh = fake_ssh(&home);
    let log = home.join("ssh.log");
    let output = tamaya(&home)
        .env("TAMAYA_SSH_BIN", ssh)
        .env("TAMAYA_FAKE_SSH_LOG", &log)
        .arg("deploy")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let script = fs::read_to_string(log).unwrap();
    assert!(script.contains("MemoryMax=512M"));
    assert!(script.contains("CPUQuota=50%"));
    assert!(script.contains("/healthz"));
    assert!(script.contains("web.example.com"));
}

#[test]
fn rollback_status_and_delete_use_worker_metadata() {
    let home = initialized("lifecycle");
    add_worker(&home);
    let ssh = fake_ssh(&home);
    let log = home.join("ssh.log");
    for args in [
        vec!["rollback", "web"],
        vec!["status", "web"],
        vec!["delete", "web", "--purge"],
    ] {
        let output = tamaya(&home)
            .env("TAMAYA_SSH_BIN", &ssh)
            .env("TAMAYA_FAKE_SSH_LOG", &log)
            .arg("--project-dir")
            .arg(&home)
            .args(args)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
    }
    let script = fs::read_to_string(log).unwrap();
    assert!(script.contains("metadata.toml"));
    assert!(script.contains("rolled back"));
    assert!(script.contains("rm -rf"));
}

#[test]
fn worker_and_lifecycle_commands_use_ssh_scripts() {
    let home = initialized("commands");
    add_worker(&home);
    let ssh = fake_ssh(&home);
    let log = home.join("ssh.log");
    for args in [
        vec!["setup"],
        vec!["check"],
        vec!["status"],
        vec!["stop", "web"],
        vec!["logs", "web"],
        vec!["maintenance", "web", "--message", "Back shortly"],
        vec!["live", "web"],
        vec!["delete", "web"],
    ] {
        let output = tamaya(&home)
            .env("TAMAYA_SSH_BIN", &ssh)
            .env("TAMAYA_FAKE_SSH_LOG", &log)
            .args(args)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
    }
    let script = fs::read_to_string(log).unwrap();
    assert!(script.contains("apt-get install"));
    assert!(script.contains("cgroup.controllers"));
    assert!(script.contains("journalctl"));
    assert!(script.contains("sudo tee \"$domain_dir/$domain_key_value.maintenance\""));
    assert!(script.contains("rebuild_domain \"$domain\""));
    assert!(
        script.contains("reverse_proxy 127.0.0.1:$write_port")
            || script.contains("reverse_proxy 127.0.0.1:$port")
    );
    assert!(script.contains("! -name data"));
}

#[test]
fn env_commands_round_trip() {
    let home = initialized("env");
    add_worker(&home);
    let ssh = env_ssh(&home);
    let log = home.join("ssh.log");
    let set = output_with_stdin(
        tamaya(&home)
            .env("TAMAYA_SSH_BIN", &ssh)
            .env("TAMAYA_FAKE_SSH_LOG", &log)
            .args(["env", "web", "set", "TOKEN", "--stdin"]),
        "secret\n",
    );
    assert!(set.status.success(), "{set:?}");
    assert!(!String::from_utf8_lossy(&set.stdout).contains("secret"));
    let list = tamaya(&home)
        .env("TAMAYA_SSH_BIN", &ssh)
        .env("TAMAYA_FAKE_SSH_LOG", &log)
        .args(["env", "web", "list"])
        .output()
        .unwrap();
    assert!(list.status.success(), "{list:?}");
    assert!(String::from_utf8_lossy(&list.stdout).contains("TOKEN"));
    assert!(!String::from_utf8_lossy(&list.stdout).contains("secret"));
    let unset = tamaya(&home)
        .env("TAMAYA_SSH_BIN", &ssh)
        .env("TAMAYA_FAKE_SSH_LOG", &log)
        .args(["env", "web", "unset", "TOKEN"])
        .output()
        .unwrap();
    assert!(unset.status.success(), "{unset:?}");
    assert!(!home.join(".local/share/tamaya/envs/web.env").exists());
    let script = fs::read_to_string(log).unwrap();
    assert!(script.contains("/etc/tamaya/apps/$app.env"));
    assert!(!script.contains("secret"));
}

#[test]
fn env_list_uses_project_app_and_worker_defaults() {
    let home = initialized("env-project-defaults");
    fs::write(
        home.join(".tamaya.toml"),
        r#"name = "project-web"
worker = "prod"
"#,
    )
    .unwrap();
    let log = home.join("ssh.log");
    let output = tamaya(&home)
        .env("TAMAYA_SSH_BIN", env_ssh(&home))
        .env("TAMAYA_FAKE_SSH_LOG", &log)
        .args(["env", "list"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stdout).contains("TOKEN"));
    let script = fs::read_to_string(log).unwrap();
    assert!(script.contains("prod"));
    assert!(script.contains("app=project-web"));
}

#[test]
fn env_commands_report_missing_worker_and_ssh_errors() {
    let home = initialized("env-errors");
    let output = output_with_stdin(
        tamaya(&home).args(["env", "--app", "web", "set", "A", "--stdin"]),
        "1\n",
    );
    assert!(!output.status.success(), "{output:?}");

    add_worker(&home);
    let output = tamaya(&home)
        .env("TAMAYA_SSH_BIN", failing_ssh(&home))
        .args(["env", "web", "list"])
        .output()
        .unwrap();
    assert!(!output.status.success(), "{output:?}");
}

#[test]
fn deploy_rejects_missing_binary() {
    let home = initialized("missing-binary");
    add_worker(&home);
    let output = tamaya(&home)
        .args(["deploy", "web", "--binary", "./missing"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("binary does not exist"));
}

#[test]
fn deploy_rejects_static_root_config() {
    let home = initialized("deploy-rejects-static-root");
    fs::write(home.join("web"), "binary").unwrap();
    fs::write(
        home.join(".tamaya.toml"),
        r#"name = "web"
worker = "prod"
binary = "./web"
static_root = "./dist"
"#,
    )
    .unwrap();
    add_worker(&home);
    let output = tamaya(&home).arg("deploy").output().unwrap();
    assert!(!output.status.success(), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("deploy does not support static_root")
    );
}

#[test]
fn publish_rejects_invalid_config_combinations() {
    let home = initialized("publish-invalid-combos");
    fs::create_dir_all(home.join("dist")).unwrap();
    fs::write(home.join("dist/index.html"), "<h1>docs</h1>").unwrap();
    add_worker(&home);

    fs::write(
        home.join(".tamaya.toml"),
        r#"name = "docs"
worker = "prod"
domain = "example.com"
binary = "./docs"
static_root = "./dist"
"#,
    )
    .unwrap();
    let output = tamaya(&home).arg("publish").output().unwrap();
    assert!(!output.status.success(), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stderr).contains("publish does not support binary"));

    fs::write(
        home.join(".tamaya.toml"),
        r#"name = "docs"
worker = "prod"
static_root = "./dist"
"#,
    )
    .unwrap();
    let output = tamaya(&home).arg("publish").output().unwrap();
    assert!(!output.status.success(), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stderr).contains("domain is required for publish"));

    fs::write(
        home.join(".tamaya.toml"),
        r#"name = "docs"
worker = "prod"
domain = "example.com"
static_root = "./dist"
path = "/"
"#,
    )
    .unwrap();
    let ssh = fake_ssh(&home);
    let log = home.join("ssh-root-path.log");
    let output = tamaya(&home)
        .env("TAMAYA_SSH_BIN", ssh)
        .env("TAMAYA_FAKE_SSH_LOG", &log)
        .arg("publish")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let script = fs::read_to_string(log).unwrap();
    assert!(
        script.contains("route_kind = \"root\"") || script.contains("route_kind = \"$route_kind\"")
    );
    assert!(script.contains("rebuild_domain \"$domain\""));
}

#[test]
fn ssh_failures_are_reported() {
    let home = initialized("ssh-failures");
    add_worker(&home);
    let ssh = failing_ssh(&home);
    let binary = home.join("web");
    fs::write(&binary, "binary").unwrap();
    for args in [
        vec!["setup"],
        vec!["logs", "web"],
        vec!["deploy", "web", "--binary", binary.to_str().unwrap()],
    ] {
        let output = tamaya(&home)
            .env("TAMAYA_SSH_BIN", &ssh)
            .args(args)
            .output()
            .unwrap();
        assert!(!output.status.success(), "{output:?}");
    }
}

#[test]
fn deploy_rejects_bad_domain() {
    let home = initialized("deploy-bad-domain");
    add_worker(&home);
    let binary = home.join("web");
    fs::write(&binary, "binary").unwrap();
    let output = tamaya(&home)
        .env("TAMAYA_SSH_BIN", fake_ssh(&home))
        .env("TAMAYA_FAKE_SSH_LOG", home.join("ssh.log"))
        .args(["deploy", "web", "--binary"])
        .arg(&binary)
        .args(["--domain", "bad domain"])
        .output()
        .unwrap();
    assert!(!output.status.success(), "{output:?}");
}

#[test]
fn missing_ssh_binary_is_reported() {
    let home = initialized("missing-ssh");
    add_worker(&home);
    let missing = home.join("does-not-exist");
    for args in [vec!["setup"], vec!["logs", "web"]] {
        let output = tamaya(&home)
            .env("TAMAYA_SSH_BIN", &missing)
            .args(args)
            .output()
            .unwrap();
        assert!(!output.status.success(), "{output:?}");
    }
}

#[test]
fn invalid_project_dir_and_ssh_output_are_reported() {
    let home = initialized("invalid-output");
    add_worker(&home);
    let missing = home.join("missing-project");
    let output = tamaya(&home)
        .arg("--project-dir")
        .arg(&missing)
        .arg("version")
        .output()
        .unwrap();
    assert!(!output.status.success(), "{output:?}");

    let ssh = invalid_utf8_ssh(&home);
    for args in [
        vec!["check"],
        vec!["rollback", "web"],
        vec!["status", "web"],
    ] {
        let output = tamaya(&home)
            .env("TAMAYA_SSH_BIN", &ssh)
            .args(args)
            .output()
            .unwrap();
        assert!(!output.status.success(), "{output:?}");
    }
}

fn initialized(name: &str) -> PathBuf {
    temp_dir(name)
}

fn add_worker(home: &Path) {
    let path = home.join(".tamaya.toml");
    let mut config = fs::read_to_string(&path).unwrap_or_default();
    if !config
        .lines()
        .any(|line| line.trim_start().starts_with("worker"))
    {
        if let Some(table_start) = config.find("\n[") {
            config.insert_str(table_start + 1, "worker = \"prod\"\n");
        } else {
            if !config.is_empty() && !config.ends_with('\n') {
                config.push('\n');
            }
            config.push_str("worker = \"prod\"\n");
        }
    }
    fs::write(path, config).unwrap();
}

fn tamaya(home: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_tamaya"));
    command
        .current_dir(home)
        .env("HOME", home)
        .env("XDG_DATA_HOME", home.join(".local/share"));
    command
}

fn output_with_stdin(command: &mut Command, input: &str) -> Output {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn fake_ssh(home: &Path) -> PathBuf {
    let path = home.join("ssh");
    fs::write(
        &path,
        r#"#!/bin/sh
printf '%s\n' "$*" >> "$TAMAYA_FAKE_SSH_LOG"
cat >/dev/null
exit 0
"#,
    )
    .unwrap();
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&path, permissions).unwrap();
    path
}

fn env_ssh(home: &Path) -> PathBuf {
    let path = home.join("env-ssh");
    fs::write(
        &path,
        r#"#!/bin/sh
printf '%s\n' "$*" >> "$TAMAYA_FAKE_SSH_LOG"
case "$*" in
  *"loading environment variables"*) printf 'TOKEN\n' ;;
  *) cat >/dev/null ;;
esac
exit 0
"#,
    )
    .unwrap();
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&path, permissions).unwrap();
    path
}

fn failing_ssh(home: &Path) -> PathBuf {
    let path = home.join("failing-ssh");
    fs::write(&path, "#!/bin/sh\necho failure >&2\nexit 1\n").unwrap();
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&path, permissions).unwrap();
    path
}

fn invalid_utf8_ssh(home: &Path) -> PathBuf {
    let path = home.join("invalid-utf8-ssh");
    fs::write(&path, "#!/bin/sh\nprintf '\\377'\n").unwrap();
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&path, permissions).unwrap();
    path
}

fn progress_then_failure_ssh(home: &Path) -> PathBuf {
    let path = home.join("progress-then-failure-ssh");
    fs::write(
        &path,
        "#!/bin/sh\nprintf '__TAMAYA_PROGRESS__remote setup stage\\n' >&2\nprintf 'remote failure\\n' >&2\nexit 1\n",
    )
    .unwrap();
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&path, permissions).unwrap();
    path
}

fn streaming_logs_ssh(home: &Path) -> PathBuf {
    let path = home.join("streaming-logs-ssh");
    fs::write(
        &path,
        "#!/bin/sh\nprintf '__TAMAYA_PROGRESS__streaming logs\\n' >&2\nprintf 'journal line\\n'\n",
    )
    .unwrap();
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&path, permissions).unwrap();
    path
}

fn temp_dir(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "tamaya-{name}-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

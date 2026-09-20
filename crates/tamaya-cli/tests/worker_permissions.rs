#![cfg(target_os = "linux")]

use std::fs;
use std::io::ErrorKind;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
#[ignore = "requires a non-root Linux user with passwordless sudo"]
fn nonroot_ssh_reads_private_metadata_and_creates_root_owned_locks() {
    let uid = Command::new("id").arg("-u").output().unwrap();
    assert_success(&uid);
    assert_ne!(
        String::from_utf8_lossy(&uid.stdout).trim(),
        "0",
        "run this test as a non-root user so it exercises the SSH privilege boundary"
    );
    let sudo = Command::new("sudo")
        .args(["-n", "id", "-u"])
        .output()
        .expect("this Linux regression test requires sudo");
    assert!(
        sudo.status.success(),
        "this Linux regression test requires passwordless sudo: {}",
        String::from_utf8_lossy(&sudo.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&sudo.stdout).trim(), "0");

    let fixture = WorkerFixture::new();
    let data = fixture.path.join("data");
    let metadata = data.join("apps/docs/metadata.toml");
    assert_root_mode(&data, 0o755);
    assert_root_mode(&metadata, 0o600);
    assert_eq!(
        fs::read(&metadata).unwrap_err().kind(),
        ErrorKind::PermissionDenied,
        "the SSH user must not be able to read the root-owned metadata directly"
    );
    let ports_lock = data.join("ports.lock");
    assert!(!ports_lock.exists());
    assert_eq!(
        fs::File::create(&ports_lock).unwrap_err().kind(),
        ErrorKind::PermissionDenied,
        "the SSH user must not be able to create the worker lock directly"
    );

    // Execute the complete status script through the same remote command as SSH.
    let status = fixture.cli().args(["status", "docs"]).output().unwrap();
    assert_success(&status);
    let status_text = String::from_utf8_lossy(&status.stdout);
    assert!(status_text.contains("docs"), "{status_text}");
    assert!(status_text.contains("20260920000000"), "{status_text}");
    assert!(status_text.contains("published/static"), "{status_text}");

    // Exercise the actual deploy prelude through its first port lock. The SSH
    // fixture stops there, before useradd, systemd or release installation.
    let deploy = fixture
        .cli()
        .env("TAMAYA_TEST_DEPLOY_PRELUDE", "1")
        .args(["deploy", "web", "--binary", "binary"])
        .output()
        .unwrap();
    assert_success(&deploy);
    assert!(
        String::from_utf8_lossy(&deploy.stdout).contains("worker locks acquired"),
        "{deploy:?}"
    );
    assert_eq!(fs::metadata(&ports_lock).unwrap().uid(), 0);
    assert_root_mode(&data.join("app-locks/web.lock"), 0o600);
    assert_root_mode(&metadata, 0o600);
}

struct WorkerFixture {
    path: PathBuf,
}

impl WorkerFixture {
    fn new() -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "tamaya-worker-permissions-{}-{stamp}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        let fixture = Self { path };
        fs::write(
            fixture.path.join(".tamaya.toml"),
            "worker = \"local-fixture\"\n",
        )
        .unwrap();
        fs::write(fixture.path.join("binary"), b"test binary\n").unwrap();

        let data = fixture.path.join("data");
        let app_dir = data.join("apps/docs");
        let directories = Command::new("sudo")
            .args([
                "-n", "install", "-d", "-o", "root", "-g", "root", "-m", "0755",
            ])
            .args([
                data.as_path(),
                data.join("apps").as_path(),
                app_dir.as_path(),
            ])
            .output()
            .unwrap();
        assert_success(&directories);
        let metadata_source = fixture.path.join("metadata-source.toml");
        fs::write(
            &metadata_source,
            format!(
                "app = \"docs\"\ncurrent = \"20260920000000\"\nprevious = \"\"\n\
                 app_type = \"published\"\nunit = \"\"\nport = 0\ndomain = \"example.com\"\n\
                 path = \"/docs\"\nroute_kind = \"path\"\nstatus = \"running\"\n\
                 health_path = \"\"\npublish_type = \"static\"\n\
                 site_dir = \"{}/releases/20260920000000/site\"\n",
                app_dir.display()
            ),
        )
        .unwrap();
        let install = Command::new("sudo")
            .args(["-n", "install", "-o", "root", "-g", "root", "-m", "0600"])
            .arg(metadata_source)
            .arg(app_dir.join("metadata.toml"))
            .output()
            .unwrap();
        assert_success(&install);

        // Preserve the CLI's privilege wrapper exactly: this fixture never adds
        // sudo. Only redirect worker paths and bound deploy to the lock prelude.
        let ssh = fixture.path.join("ssh");
        fs::write(
            &ssh,
            r#"#!/usr/bin/python3
import os
import shlex
import sys

assert os.geteuid() != 0, "SSH fixture must start as the non-root user"
assert len(sys.argv) == 3 and sys.argv[1] == "local-fixture"
command = shlex.split(sys.argv[2])
assert command[-3:-1] == ["sh", "-lc"], command[:-1]
script = command[-1]
root = os.path.dirname(os.path.realpath(__file__))
script = script.replace("/var/lib/tamaya", root + "/data")
script = script.replace("/etc/caddy/conf.d", root + "/caddy")
if os.environ.get("TAMAYA_TEST_DEPLOY_PRELUDE") == "1":
    marker = '\nflock 8\n'
    assert script.count(marker) == 1, "deploy lock boundary changed"
    script = script.split(marker)[0] + marker
    script += "cat >/dev/null\nprintf 'worker locks acquired\\n'\n"
command[-1] = script
os.execvp(command[0], command)
"#,
        )
        .unwrap();
        fs::set_permissions(&ssh, fs::Permissions::from_mode(0o755)).unwrap();
        fixture
    }

    fn cli(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_tamaya"));
        command
            .current_dir(&self.path)
            .env("TAMAYA_SSH_BIN", self.path.join("ssh"));
        command
    }
}

impl Drop for WorkerFixture {
    fn drop(&mut self) {
        // Every privileged file is confined to this unique fixture directory.
        let cleanup = Command::new("sudo")
            .args(["-n", "rm", "-rf", "--"])
            .arg(&self.path)
            .output();
        match cleanup {
            Ok(output) if output.status.success() => {}
            result if std::thread::panicking() => {
                eprintln!("failed to clean {}: {result:?}", self.path.display());
            }
            result => panic!("failed to clean {}: {result:?}", self.path.display()),
        }
    }
}

fn assert_success(output: &Output) {
    assert!(output.status.success(), "{output:?}");
}

fn assert_root_mode(path: &Path, mode: u32) {
    let metadata = fs::metadata(path).unwrap();
    assert_eq!(metadata.uid(), 0, "{} owner", path.display());
    assert_eq!(metadata.mode() & 0o777, mode, "{} mode", path.display());
}

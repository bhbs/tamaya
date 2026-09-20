#![cfg(target_os = "linux")]

use std::fs;
use std::io::{ErrorKind, Write};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
#[ignore = "requires a non-root Linux user with passwordless sudo and running systemd"]
fn environment_values_round_trip_through_real_systemd() {
    let uid = Command::new("id").arg("-u").output().unwrap();
    assert_success(&uid);
    assert_ne!(
        String::from_utf8_lossy(&uid.stdout).trim(),
        "0",
        "run this test as a non-root user to exercise the SSH privilege boundary"
    );
    let sudo = Command::new("sudo")
        .args(["-n", "id", "-u"])
        .output()
        .expect("this Linux regression test requires passwordless sudo");
    assert_success(&sudo);
    assert_eq!(String::from_utf8_lossy(&sudo.stdout).trim(), "0");

    for legacy_tail in [
        "",
        "LEGACY_CONTINUATION=trailing\\\n",
        "LEGACY_QUOTE=\"unterminated\n",
    ] {
        verify_values(legacy_tail);
    }
}

fn verify_values(legacy_tail: &str) {
    let fixture = WorkerFixture::new(legacy_tail);
    let values = [
        ("EMPTY", ""),
        ("SPACES", "  leading and trailing  "),
        ("SINGLE_QUOTE", "'single quoted'"),
        ("DOUBLE_QUOTE", "\"double quoted\""),
        ("BACKSLASH", r"C:\path\to\file\"),
        ("LITERAL_NEWLINE", r"line one\nline two"),
        ("DOLLAR", r"$HOME ${TOKEN} $(printf unsafe)"),
        ("BACKTICK", "`printf unsafe`"),
        ("TABS", "\tleading\tmiddle\ttrailing\t"),
        ("UNICODE", "日本語🙂 café"),
        ("COMMENTS", "#value ; literal = stays"),
        ("REPLACED", "new value with \\ and \""),
        ("_UNDER_2", "valid"),
    ];
    for (key, value) in values {
        let mut child = fixture
            .cli()
            .args(["env", "web", "set", key, "--stdin"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(value.as_bytes())
            .unwrap();
        assert_success(&child.wait_with_output().unwrap());
    }

    let env_path = fixture.env_path();
    let metadata = fs::metadata(&env_path).unwrap();
    assert_eq!(metadata.uid(), 0, "environment file owner");
    assert_eq!(metadata.mode() & 0o777, 0o600, "environment file mode");
    assert_eq!(
        fs::read(&env_path).unwrap_err().kind(),
        ErrorKind::PermissionDenied,
        "the SSH user must not be able to read the environment file directly"
    );
    let stored = Command::new("sudo")
        .args(["-n", "cat"])
        .arg(&env_path)
        .output()
        .unwrap();
    assert_success(&stored);
    let stored = String::from_utf8(stored.stdout).unwrap();
    assert!(
        stored.lines().any(|line| line == "LEGACY=plain"),
        "existing unquoted entries must remain compatible"
    );
    assert_eq!(
        stored
            .lines()
            .filter(|line| line.starts_with("REPLACED="))
            .count(),
        1,
        "setting a legacy entry must replace it rather than append a duplicate"
    );

    // Use systemd's actual EnvironmentFile parser, independently of the CLI's
    // encoder and our file assertions. Values are dummy data, never real secrets.
    let output = Command::new("sudo")
        .args([
            "-n",
            "systemd-run",
            "--quiet",
            "--wait",
            "--pipe",
            "--collect",
            "--service-type=exec",
        ])
        .arg(format!("--property=EnvironmentFile={}", env_path.display()))
        .args([
            "/usr/bin/python3",
            "-c",
            "import os, sys; sys.stdout.buffer.write(b''.join(key.encode() + b'=' + os.environ[key].encode('utf-8') + b'\\0' for key in sys.argv[1:]))",
            "LEGACY",
        ])
        .args(values.iter().map(|(key, _)| key))
        .output()
        .expect("this Linux regression test requires a running systemd manager");
    assert_success(&output);
    let mut expected = b"LEGACY=plain\0".to_vec();
    for (key, value) in values {
        expected.extend_from_slice(key.as_bytes());
        expected.push(b'=');
        expected.extend_from_slice(value.as_bytes());
        expected.push(0);
    }
    assert_eq!(output.stdout, expected, "values received by the service");
}

struct WorkerFixture {
    path: PathBuf,
}

impl WorkerFixture {
    fn new(legacy_tail: &str) -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("tamaya-env-systemd-{}-{stamp}", std::process::id()));
        fs::create_dir(&path).unwrap();
        let fixture = Self { path };
        fs::write(
            fixture.path.join(".tamaya.toml"),
            "worker = \"local-fixture\"\n",
        )
        .unwrap();
        let directories = Command::new("sudo")
            .args([
                "-n", "install", "-d", "-o", "root", "-g", "root", "-m", "0755",
            ])
            .arg(fixture.path.join("data"))
            .arg(fixture.path.join("env"))
            .output()
            .unwrap();
        assert_success(&directories);
        let legacy = fixture.path.join("legacy.env");
        fs::write(
            &legacy,
            format!("LEGACY=plain\nREPLACED=old\n{legacy_tail}"),
        )
        .unwrap();
        let install = Command::new("sudo")
            .args(["-n", "install", "-o", "root", "-g", "root", "-m", "0600"])
            .arg(legacy)
            .arg(fixture.env_path())
            .output()
            .unwrap();
        assert_success(&install);

        // Keep the CLI's sudo wrapper and the worker shell intact. Only remap
        // worker storage into this fixture, with no SSH or systemd mock.
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
assert command[:4] == ["sudo", "-n", "sh", "-lc"], command[:-1]
assert len(command) == 5
root = os.path.dirname(os.path.realpath(__file__))
command[-1] = command[-1].replace("/var/lib/tamaya", root + "/data")
command[-1] = command[-1].replace("/etc/tamaya/apps", root + "/env")
os.execvp(command[0], command)
"#,
        )
        .unwrap();
        fs::set_permissions(&ssh, fs::Permissions::from_mode(0o755)).unwrap();
        fixture
    }

    fn env_path(&self) -> PathBuf {
        self.path.join("env/web.env")
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

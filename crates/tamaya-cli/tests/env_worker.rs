mod support;

use std::fs;
use std::io::Write;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use support::WorkerFixture;

// Exercise complete generated worker scripts with real file-descriptor locks.
// Pause at rename so a concurrent update must wait before reading the old file.
const ENV_COMMANDS: &str = r#"#!/usr/bin/env python3
import fcntl
import os
from pathlib import Path
import sys
import time

root = Path(os.environ["TAMAYA_TEST_WORKER"])
operation = os.environ["TAMAYA_TEST_OPERATION"]
command = Path(sys.argv[0]).name
args = sys.argv[1:]

def event(name):
    (root / (operation + "-" + name)).touch()

if command == "flock":
    descriptor = int(args[0])
    if descriptor == 6:
        event("lock-attempted")
    fcntl.flock(descriptor, fcntl.LOCK_EX)
    if descriptor == 6:
        event("lock-acquired")
elif command == "mv":
    if Path(args[-1]) == root / "env/web.env":
        event("rename-reached")
        if (root / (operation + "-pause-rename")).exists():
            deadline = time.monotonic() + 15
            while not (root / (operation + "-resume-rename")).exists():
                if time.monotonic() > deadline:
                    raise RuntimeError("timed out waiting to resume env rename")
                time.sleep(0.01)
        if (root / (operation + "-fail-rename")).exists():
            print("injected env rename failure", file=sys.stderr)
            sys.exit(1)
    os.execv("/bin/mv", ["mv", *args])
elif command == "install":
    # Ownership is exercised by the separate Linux sudo test. Preserve the
    # actual install, permissions, and rename here without requiring root.
    filtered = []
    while args:
        item = args.pop(0)
        if item in ("-o", "-g"):
            args.pop(0)
        else:
            filtered.append(item)
    os.execv("/usr/bin/install", ["install", *filtered])
else:
    raise AssertionError(command)
"#;

struct EnvWorker {
    worker: WorkerFixture,
}

impl EnvWorker {
    fn new(initial: &str) -> Self {
        let worker = WorkerFixture::new("process");
        fs::create_dir(worker.path.join("env")).unwrap();
        let env_path = worker.path.join("env/web.env");
        fs::write(&env_path, initial).unwrap();
        fs::set_permissions(env_path, fs::Permissions::from_mode(0o600)).unwrap();
        let helper = worker.path.join("bin/env-commands.py");
        fs::write(&helper, ENV_COMMANDS).unwrap();
        fs::set_permissions(helper, fs::Permissions::from_mode(0o755)).unwrap();
        for command in ["flock", "mv", "install"] {
            let target = worker.path.join("bin").join(command);
            if target.symlink_metadata().is_ok() {
                fs::remove_file(&target).unwrap();
            }
            symlink("env-commands.py", target).unwrap();
        }
        Self { worker }
    }

    fn spawn(&self, operation: &str, args: &[&str], input: &str) -> Running {
        let path = std::env::join_paths(
            std::iter::once(self.worker.path.join("bin"))
                .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
        )
        .unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_tamaya"))
            .current_dir(&self.worker.path)
            .env("PATH", path)
            .env("TAMAYA_SSH_BIN", self.worker.path.join("bin/ssh"))
            .env("TAMAYA_TEST_WORKER", &self.worker.path)
            .env("TAMAYA_TEST_OPERATION", operation)
            .args(args)
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
        Running(Some(child))
    }

    fn mark(&self, name: &str) {
        fs::write(self.worker.path.join(name), "").unwrap();
    }

    fn event(&self, name: &str) -> bool {
        self.worker.path.join(name).exists()
    }

    fn wait_for_any(&self, names: &[&str]) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !names.iter().any(|name| self.event(name)) {
            assert!(Instant::now() < deadline, "timed out waiting for {names:?}");
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn contents(&self) -> String {
        fs::read_to_string(self.worker.path.join("env/web.env")).unwrap()
    }

    fn assert_private_without_temporary_files(&self) {
        let env_dir = self.worker.path.join("env");
        let entries: Vec<_> = fs::read_dir(&env_dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(entries, ["web.env"]);
        assert_eq!(
            fs::metadata(env_dir.join("web.env"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}

// Kill a paused CLI on assertion failure; the worker's bounded barrier also
// guarantees that its child shell cannot remain alive indefinitely.
struct Running(Option<Child>);

impl Running {
    fn wait(mut self) -> Output {
        self.0.take().unwrap().wait_with_output().unwrap()
    }

    fn success(self) {
        let output = self.wait();
        assert!(output.status.success(), "{output:?}");
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        if let Some(child) = &mut self.0 {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[test]
fn concurrent_set_preserves_both_updates() {
    let worker = EnvWorker::new("KEEP=original\n");
    worker.mark("first-pause-rename");
    let first = worker.spawn("first", &["env", "web", "set", "FIRST", "--stdin"], "one");
    worker.wait_for_any(&["first-rename-reached"]);
    assert_eq!(worker.contents(), "KEEP=original\n");
    for entry in fs::read_dir(worker.worker.path.join("env")).unwrap() {
        assert_eq!(
            entry.unwrap().metadata().unwrap().permissions().mode() & 0o777,
            0o600,
            "both the published env file and its pending replacement must be private"
        );
    }
    let second = worker.spawn("second", &["env", "web", "set", "SECOND", "--stdin"], "two");
    worker.wait_for_any(&["second-lock-attempted", "second-rename-reached"]);
    worker.mark("first-resume-rename");
    first.success();
    second.success();
    assert_eq!(
        worker.contents(),
        "SECOND=\"two\"\nFIRST=\"one\"\nKEEP=original\n"
    );
    worker.assert_private_without_temporary_files();
}

#[test]
fn concurrent_set_and_unset_do_not_resurrect_removed_keys() {
    let worker = EnvWorker::new("REMOVE=old\nKEEP=original\n");
    worker.mark("first-pause-rename");
    let first = worker.spawn("first", &["env", "web", "set", "ADDED", "--stdin"], "new");
    worker.wait_for_any(&["first-rename-reached"]);
    let second = worker.spawn("second", &["env", "web", "unset", "REMOVE"], "");
    worker.wait_for_any(&["second-lock-attempted", "second-rename-reached"]);
    worker.mark("first-resume-rename");
    first.success();
    second.success();
    assert_eq!(worker.contents(), "ADDED=\"new\"\nKEEP=original\n");
    worker.assert_private_without_temporary_files();
}

#[test]
fn delete_waits_for_pending_env_update_and_removes_its_result() {
    let worker = EnvWorker::new("KEEP=original\n");
    worker.mark("first-pause-rename");
    let first = worker.spawn("first", &["env", "web", "set", "ADDED", "--stdin"], "new");
    worker.wait_for_any(&["first-rename-reached"]);
    let second = worker.spawn("second", &["delete", "web"], "");
    worker.wait_for_any(&["second-lock-attempted"]);
    let first_holds_lock = worker.event("first-lock-acquired");
    worker.mark("first-resume-rename");
    first.success();
    second.success();
    assert!(
        first_holds_lock,
        "env update must share the app lifecycle lock"
    );
    assert!(!worker.worker.path.join("env/web.env").exists());
}

#[test]
fn failed_env_rename_preserves_old_file_and_cleans_private_temporary_files() {
    for args in [
        vec!["env", "web", "set", "ADDED", "--stdin"],
        vec!["env", "web", "unset", "REMOVE"],
    ] {
        let initial = "REMOVE=old\nKEEP=original\n";
        let worker = EnvWorker::new(initial);
        worker.mark("update-fail-rename");
        let output = worker.spawn("update", &args, "new").wait();
        assert!(!output.status.success(), "{args:?}: {output:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("injected env rename failure"),
            "{args:?}: {output:?}"
        );
        assert_eq!(worker.contents(), initial, "{args:?}");
        worker.assert_private_without_temporary_files();
    }
}

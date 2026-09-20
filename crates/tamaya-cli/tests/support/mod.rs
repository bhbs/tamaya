#![allow(dead_code)] // Integration suites use different subsets of this fixture.

use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub const CURRENT: &str = "20260920000000";
pub const PREVIOUS: &str = "20260919000000";
static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

pub struct WorkerFixture {
    pub path: PathBuf,
}

#[derive(Debug, PartialEq)]
pub struct WorkerSnapshot {
    metadata: Option<String>,
    route: Option<String>,
    merged_route: Option<String>,
    maintenance: Option<String>,
    maintenance_page: Option<String>,
    current: Option<PathBuf>,
    previous: Option<PathBuf>,
    active_services: Vec<String>,
}

impl WorkerFixture {
    pub fn new(app_type: &str) -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let sequence = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "tamaya-lifecycle-{}-{stamp}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        let fixture = Self { path };
        for directory in [
            "bin",
            "data/apps/web/releases",
            "data/caddy-routes",
            "data/caddy-domains",
            "caddy/conf.d",
            "systemd",
            "active",
        ] {
            fs::create_dir_all(fixture.path.join(directory)).unwrap();
        }
        fs::write(
            fixture.path.join(".tamaya.toml"),
            "worker = \"local-fixture\"\n",
        )
        .unwrap();
        fs::write(
            fixture.path.join("bin/worker.py"),
            include_str!("../fixtures/lifecycle_worker.py"),
        )
        .unwrap();
        fs::set_permissions(
            fixture.path.join("bin/worker.py"),
            fs::Permissions::from_mode(0o755),
        )
        .unwrap();
        for command in [
            "ssh",
            "systemctl",
            "caddy",
            "ss",
            "curl",
            "ln",
            "id",
            "cp",
            "mv",
            "userdel",
        ] {
            symlink("worker.py", fixture.path.join("bin").join(command)).unwrap();
        }
        // Sequential fixture operations do not need host ownership or locks.
        // All file operations and shell state transitions remain real.
        for (name, source) in [
            ("sudo", "#!/bin/sh\nexec \"$@\"\n"),
            ("chown", "#!/bin/sh\nexit 0\n"),
            ("flock", "#!/bin/sh\nexit 0\n"),
        ] {
            let target = fixture.path.join("bin").join(name);
            fs::write(&target, source).unwrap();
            fs::set_permissions(target, fs::Permissions::from_mode(0o755)).unwrap();
        }

        for (release, port) in [(CURRENT, 20000), (PREVIOUS, 20002)] {
            fs::create_dir_all(
                fixture
                    .path
                    .join(format!("data/apps/web/releases/{release}/site")),
            )
            .unwrap();
            if app_type == "process" {
                fs::write(
                    fixture
                        .path
                        .join(format!("systemd/tamaya-web-{release}.service")),
                    format!("[Service]\nEnvironment=PORT={port}\n"),
                )
                .unwrap();
            }
        }
        let (unit, port, health_path, publish_type, site_dir) = if app_type == "process" {
            let unit = format!("tamaya-web-{CURRENT}.service");
            fs::write(fixture.path.join("active").join(&unit), "").unwrap();
            (unit, 20000, "/health", "", String::new())
        } else {
            (
                String::new(),
                0,
                "",
                "static",
                fixture
                    .path
                    .join(format!("data/apps/web/releases/{CURRENT}/site"))
                    .display()
                    .to_string(),
            )
        };
        let metadata = format!(
            "app = \"web\"\ncurrent = \"{CURRENT}\"\nprevious = \"{PREVIOUS}\"\napp_type = \"{app_type}\"\nunit = \"{unit}\"\nport = {port}\ndomain = \"example.com\"\npath = \"/web\"\nroute_kind = \"path\"\nstatus = \"running\"\nhealth_path = \"{health_path}\"\nhealth_retries = {}\nhealth_timeout = {}\nhealth_interval = 0\npublish_type = \"{publish_type}\"\nsite_dir = \"{site_dir}\"\n",
            usize::from(app_type == "process"),
            usize::from(app_type == "process")
        );
        fs::write(fixture.path.join("data/apps/web/metadata.toml"), metadata).unwrap();
        symlink(
            format!("releases/{CURRENT}"),
            fixture.path.join("data/apps/web/current"),
        )
        .unwrap();
        symlink(
            format!("releases/{PREVIOUS}"),
            fixture.path.join("data/apps/web/previous"),
        )
        .unwrap();
        let handler = if app_type == "process" {
            "    reverse_proxy 127.0.0.1:20000\n".to_string()
        } else {
            format!(
                "    root * {site_dir}\n    try_files {{path}} {{path}}.html {{path}}/ /404.html\n    file_server\n"
            )
        };
        let route = format!("@tamaya_web path /web /web/*\nhandle @tamaya_web {{\n{handler}}}\n");
        fs::write(fixture.path.join("data/caddy-routes/web.caddy"), &route).unwrap();
        let merged = format!(
            "example.com {{\n{}}}\n",
            route
                .lines()
                .map(|line| format!("    {line}\n"))
                .collect::<String>()
        );
        fs::write(fixture.path.join("caddy/conf.d/example.com.caddy"), merged).unwrap();
        fixture
    }

    pub fn run(&self, args: &[&str]) -> Output {
        let path = std::env::join_paths(
            std::iter::once(self.path.join("bin"))
                .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
        )
        .unwrap();
        Command::new(env!("CARGO_BIN_EXE_tamaya"))
            .current_dir(&self.path)
            .env("PATH", path)
            .env("TAMAYA_SSH_BIN", self.path.join("bin/ssh"))
            .env("TAMAYA_TEST_WORKER", &self.path)
            .args(args)
            .output()
            .unwrap()
    }

    pub fn success(&self, args: &[&str]) {
        let output = self.run(args);
        assert!(output.status.success(), "{args:?}: {output:?}");
    }

    pub fn metadata(&self, key: &str) -> String {
        let metadata = fs::read_to_string(self.path.join("data/apps/web/metadata.toml")).unwrap();
        let table: toml::Table = metadata.parse().unwrap();
        match &table[key] {
            toml::Value::String(value) => value.clone(),
            value => value.to_string(),
        }
    }

    pub fn link(&self, name: &str) -> PathBuf {
        fs::read_link(self.path.join("data/apps/web").join(name)).unwrap()
    }

    pub fn active_services(&self) -> Vec<String> {
        let mut active: Vec<_> = fs::read_dir(self.path.join("active"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        active.sort();
        active
    }

    pub fn snapshot(&self) -> WorkerSnapshot {
        let read = |path: &str| match fs::read_to_string(self.path.join(path)) {
            Ok(value) => Some(value),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => panic!("{path}: {error}"),
        };
        WorkerSnapshot {
            metadata: read("data/apps/web/metadata.toml"),
            route: read("data/caddy-routes/web.caddy"),
            merged_route: read("caddy/conf.d/example.com.caddy"),
            maintenance: read("data/caddy-domains/example.com.maintenance"),
            maintenance_page: read("data/static/maintenance/example.com/index.html"),
            current: fs::read_link(self.path.join("data/apps/web/current")).ok(),
            previous: fs::read_link(self.path.join("data/apps/web/previous")).ok(),
            active_services: self.active_services(),
        }
    }
}

impl Drop for WorkerFixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.path).unwrap();
    }
}

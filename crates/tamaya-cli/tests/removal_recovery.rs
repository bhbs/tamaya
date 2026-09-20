mod support;

use std::fs;
use std::path::PathBuf;

use support::{CURRENT, PREVIOUS, WorkerFixture};

fn prepare_app_files(worker: &WorkerFixture) -> Vec<(PathBuf, String)> {
    let files = [
        (
            format!("data/apps/web/releases/{CURRENT}/app"),
            "current binary",
        ),
        (
            format!("data/apps/web/releases/{PREVIOUS}/app"),
            "previous binary",
        ),
        (
            "data/apps/web/data/records.db".to_string(),
            "persistent application data",
        ),
        ("env/web.env".to_string(), "DATABASE_TOKEN=preserve-me\n"),
    ];
    files
        .into_iter()
        .map(|(relative, contents)| {
            let path = worker.path.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, contents).unwrap();
            (path, contents.to_string())
        })
        .collect()
}

fn assert_files_preserved(files: &[(PathBuf, String)]) {
    for (path, contents) in files {
        let actual = fs::read_to_string(path).unwrap_or_else(|error| panic!("{path:?}: {error}"));
        assert_eq!(actual, *contents, "{path:?}");
    }
}

fn fail_next(worker: &WorkerFixture, stage: &str) -> PathBuf {
    let flag = worker.path.join(format!("fail-{stage}"));
    fs::write(&flag, "fail next call").unwrap();
    flag
}

#[test]
fn failed_stop_and_delete_preserve_routes_for_later_domain_rebuilds() {
    for app_type in ["process", "published"] {
        for command in ["stop", "delete"] {
            for failure in ["validate", "reload"] {
                let worker = WorkerFixture::new(app_type);
                let files = prepare_app_files(&worker);
                let before = worker.snapshot();
                let flag = fail_next(&worker, failure);

                let output = worker.run(&[command, "web"]);
                assert!(
                    !output.status.success(),
                    "{app_type} {command} {failure}: {output:?}"
                );
                assert!(!flag.exists(), "failure injection was not reached");
                assert_eq!(worker.snapshot(), before, "{app_type} {command} {failure}");
                assert_files_preserved(&files);

                // Domain live rebuilds existing snippets without recreating the
                // app snippet, so a lost route cannot be hidden by this check.
                worker.success(&["maintenance", "--domain", "example.com"]);
                worker.success(&["live", "--domain", "example.com"]);
                assert_eq!(worker.snapshot(), before, "{app_type} {command} {failure}");
            }
        }
    }
}

#[test]
fn failed_stop_metadata_write_preserves_the_running_service_and_route() {
    for app_type in ["process", "published"] {
        let worker = WorkerFixture::new(app_type);
        let before = worker.snapshot();
        let flag = fail_next(&worker, "metadata-write");

        let output = worker.run(&["stop", "web"]);
        assert!(!output.status.success(), "{app_type}: {output:?}");
        assert!(!flag.exists(), "failure injection was not reached");
        assert_eq!(worker.snapshot(), before, "{app_type}");
        let log = fs::read_to_string(worker.path.join("systemctl.log")).unwrap_or_default();
        assert!(!log.contains("disable --now"), "{log}");

        worker.success(&["maintenance", "--domain", "example.com"]);
        worker.success(&["live", "--domain", "example.com"]);
        assert_eq!(worker.snapshot(), before, "{app_type}");
    }
}

#[test]
fn failed_purge_preserves_releases_data_and_domain_maintenance() {
    for app_type in ["process", "published"] {
        for failure in ["validate", "reload"] {
            let worker = WorkerFixture::new(app_type);
            let files = prepare_app_files(&worker);
            let running = worker.snapshot();
            worker.success(&["maintenance", "--domain", "example.com"]);
            let before = worker.snapshot();
            let flag = fail_next(&worker, failure);

            let output = worker.run(&["delete", "web", "--purge"]);
            assert!(!output.status.success(), "{app_type} {failure}: {output:?}");
            assert!(!flag.exists(), "failure injection was not reached");
            assert_eq!(worker.snapshot(), before, "{app_type} {failure}");
            assert_files_preserved(&files);

            worker.success(&["live", "--domain", "example.com"]);
            assert_eq!(worker.snapshot(), running, "{app_type} {failure}");
        }
    }
}

#[test]
fn failed_removal_preserves_legacy_caddy_files() {
    for command in ["stop", "delete"] {
        let failures = if command == "stop" {
            vec!["validate", "reload", "metadata-write"]
        } else {
            vec!["validate", "reload"]
        };
        for failure in failures {
            let worker = WorkerFixture::new("process");
            let mut legacy_files = Vec::new();
            for name in ["web", "legacy-domain"] {
                let path = worker.path.join(format!("caddy/conf.d/{name}.caddy"));
                let contents = "example.com {\n    reverse_proxy 127.0.0.1:20000\n}\n";
                fs::write(&path, contents).unwrap();
                legacy_files.push((path, contents.to_string()));
            }
            let before = worker.snapshot();
            let flag = fail_next(&worker, failure);

            let output = worker.run(&[command, "web"]);
            assert!(!output.status.success(), "{command} {failure}: {output:?}");
            assert!(!flag.exists(), "failure injection was not reached");
            assert_eq!(worker.snapshot(), before, "{command} {failure}");
            assert_files_preserved(&legacy_files);
        }
    }
}

#[test]
fn successful_stop_commits_stopped_state_before_stopping_services() {
    for app_type in ["process", "published"] {
        let worker = WorkerFixture::new(app_type);
        let files = prepare_app_files(&worker);

        worker.success(&["stop", "web"]);
        assert_eq!(worker.metadata("status"), "stopped");
        assert!(worker.active_services().is_empty());
        assert!(!worker.path.join("data/caddy-routes/web.caddy").exists());
        assert!(!worker.path.join("caddy/conf.d/example.com.caddy").exists());
        assert_files_preserved(&files);
    }
}

#[test]
fn successful_delete_removes_routes_services_and_only_purges_data_when_requested() {
    for app_type in ["process", "published"] {
        for purge in [false, true] {
            let worker = WorkerFixture::new(app_type);
            prepare_app_files(&worker);
            worker.success(&["maintenance", "--domain", "example.com"]);
            let args = if purge {
                vec!["delete", "web", "--purge"]
            } else {
                vec!["delete", "web"]
            };

            worker.success(&args);
            assert!(worker.active_services().is_empty());
            assert_eq!(
                fs::read_dir(worker.path.join("systemd")).unwrap().count(),
                0
            );
            for path in [
                "data/apps/web/metadata.toml",
                "data/apps/web/releases",
                "data/caddy-routes/web.caddy",
                "caddy/conf.d/example.com.caddy",
                "data/caddy-domains/example.com.maintenance",
                "data/static/maintenance/example.com",
                "env/web.env",
            ] {
                assert!(
                    !worker.path.join(path).exists(),
                    "{app_type} purge={purge} {path}"
                );
            }
            if purge {
                assert!(!worker.path.join("data/apps/web").exists());
            } else {
                assert_eq!(
                    fs::read_to_string(worker.path.join("data/apps/web/data/records.db")).unwrap(),
                    "persistent application data"
                );
            }
        }
    }
}

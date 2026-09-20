mod support;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;
use support::{CURRENT, WorkerFixture};

fn prepare_upload(worker: &WorkerFixture, app_type: &str) {
    fs::write(
        worker.path.join(".tamaya.toml"),
        "worker = \"local-fixture\"\ndomain = \"example.com\"\npath = \"/web\"\n",
    )
    .unwrap();
    if app_type == "process" {
        fs::write(worker.path.join("upload"), "fixture binary").unwrap();
    } else {
        fs::create_dir_all(worker.path.join("upload/web")).unwrap();
        fs::write(worker.path.join("upload/web/index.html"), "fixture site").unwrap();
    }
}

fn upload(worker: &WorkerFixture, app_type: &str) -> Output {
    if app_type == "process" {
        worker.run(&["deploy", "web", "--binary", "upload"])
    } else {
        worker.run(&["publish", "web", "--static-root", "upload"])
    }
}

fn entries(path: &Path) -> Vec<PathBuf> {
    let mut entries: Vec<_> = fs::read_dir(path)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    entries.sort();
    entries
}

fn assert_failed_upload_restores(worker: &WorkerFixture, app_type: &str, failure: &str) {
    let before = worker.snapshot();
    let releases = entries(&worker.path.join("data/apps/web/releases"));
    let units = entries(&worker.path.join("systemd"));
    let failure_flag = worker.path.join(format!("fail-{failure}"));
    fs::write(&failure_flag, "fail next call").unwrap();

    let output = upload(worker, app_type);
    assert!(!output.status.success(), "{app_type} {failure}: {output:?}");
    assert!(
        !failure_flag.exists(),
        "{app_type} {failure} injection was not reached: {output:?}"
    );
    assert_eq!(worker.snapshot(), before, "{app_type} {failure}");
    assert_eq!(
        entries(&worker.path.join("data/apps/web/releases")),
        releases,
        "failed release must be removed"
    );
    assert_eq!(entries(&worker.path.join("systemd")), units);
}

#[test]
fn failed_deploy_and_publish_restore_running_and_stopped_apps() {
    for app_type in ["process", "published"] {
        for stopped in [false, true] {
            for failure in [
                "metadata-write",
                "validate",
                "reload",
                "current-link",
                "previous-link",
            ] {
                let worker = WorkerFixture::new(app_type);
                prepare_upload(&worker, app_type);
                if stopped {
                    worker.success(&["stop", "web"]);
                }
                assert_failed_upload_restores(&worker, app_type, failure);
            }
        }
    }
}

#[test]
fn failed_first_deploy_and_publish_leave_no_release_state() {
    for app_type in ["process", "published"] {
        for failure in ["metadata-write", "validate", "reload", "current-link"] {
            let worker = WorkerFixture::new(app_type);
            prepare_upload(&worker, app_type);
            for directory in ["data/apps/web", "systemd", "active"] {
                fs::remove_dir_all(worker.path.join(directory)).unwrap();
                fs::create_dir_all(worker.path.join(directory)).unwrap();
            }
            fs::create_dir_all(worker.path.join("data/apps/web/releases")).unwrap();
            fs::remove_file(worker.path.join("data/caddy-routes/web.caddy")).unwrap();
            fs::remove_file(worker.path.join("caddy/conf.d/example.com.caddy")).unwrap();
            assert_failed_upload_restores(&worker, app_type, failure);
        }
    }
}

#[test]
fn failed_deploy_and_publish_preserve_missing_previous_links() {
    for app_type in ["process", "published"] {
        let worker = WorkerFixture::new(app_type);
        prepare_upload(&worker, app_type);
        fs::remove_file(worker.path.join("data/apps/web/previous")).unwrap();
        assert_failed_upload_restores(&worker, app_type, "previous-link");
    }
}

#[test]
fn deploy_and_publish_abort_when_recovery_backup_fails() {
    for app_type in ["process", "published"] {
        for failure in ["metadata-backup", "route-backup"] {
            let worker = WorkerFixture::new(app_type);
            prepare_upload(&worker, app_type);
            assert_failed_upload_restores(&worker, app_type, failure);
        }
    }
}

#[test]
fn failed_deploy_and_publish_restore_legacy_standalone_caddy_routes() {
    for app_type in ["process", "published"] {
        for failure in ["validate", "reload", "current-link"] {
            let worker = WorkerFixture::new(app_type);
            prepare_upload(&worker, app_type);
            let merged_path = worker.path.join("caddy/conf.d/example.com.caddy");
            let legacy_path = worker.path.join("caddy/conf.d/web.caddy");
            let original_route = fs::read_to_string(&merged_path).unwrap();
            fs::rename(merged_path, &legacy_path).unwrap();
            fs::remove_file(worker.path.join("data/caddy-routes/web.caddy")).unwrap();

            assert_failed_upload_restores(&worker, app_type, failure);
            assert_eq!(fs::read_to_string(legacy_path).unwrap(), original_route);
        }
    }
}

#[test]
fn failed_first_deploy_and_publish_preserve_another_apps_only_legacy_route() {
    for app_type in ["process", "published"] {
        for failure in ["validate", "current-link"] {
            let worker = WorkerFixture::new(app_type);
            prepare_upload(&worker, app_type);
            fs::write(
                worker.path.join(".tamaya.toml"),
                "worker = \"local-fixture\"\ndomain = \"example.com\"\npath = \"/new\"\n",
            )
            .unwrap();
            if app_type == "published" {
                fs::rename(
                    worker.path.join("upload/web"),
                    worker.path.join("upload/new"),
                )
                .unwrap();
            }

            // Begin with one valid old app served by its standalone Caddy
            // file. The new app uses a separate path on that same domain.
            fs::rename(
                worker.path.join("data/apps/web"),
                worker.path.join("data/apps/old"),
            )
            .unwrap();
            fs::create_dir_all(worker.path.join("data/apps/web/releases")).unwrap();
            let old_metadata_path = worker.path.join("data/apps/old/metadata.toml");
            let old_metadata = fs::read_to_string(&old_metadata_path)
                .unwrap()
                .replace("app = \"web\"", "app = \"old\"")
                .replace("tamaya-web-", "tamaya-old-")
                .replace("/apps/web/", "/apps/old/");
            fs::write(&old_metadata_path, &old_metadata).unwrap();
            for directory in ["systemd", "active"] {
                for path in entries(&worker.path.join(directory)) {
                    let name = path.file_name().unwrap().to_str().unwrap();
                    fs::rename(
                        &path,
                        path.with_file_name(name.replace("tamaya-web-", "tamaya-old-")),
                    )
                    .unwrap();
                }
            }
            let merged_path = worker.path.join("caddy/conf.d/example.com.caddy");
            let legacy_path = worker.path.join("caddy/conf.d/old.caddy");
            let legacy_route = fs::read_to_string(&merged_path)
                .unwrap()
                .replace("/apps/web/", "/apps/old/");
            fs::write(&legacy_path, &legacy_route).unwrap();
            fs::remove_file(merged_path).unwrap();
            fs::remove_file(worker.path.join("data/caddy-routes/web.caddy")).unwrap();

            assert_failed_upload_restores(&worker, app_type, failure);
            assert_eq!(
                fs::read_to_string(&old_metadata_path).unwrap(),
                old_metadata
            );
            assert_eq!(fs::read_to_string(&legacy_path).unwrap(), legacy_route);
        }
    }
}

#[test]
fn failed_recovery_keeps_the_candidate_available_for_manual_repair() {
    for app_type in ["process", "published"] {
        let worker = WorkerFixture::new(app_type);
        prepare_upload(&worker, app_type);
        let original_releases = entries(&worker.path.join("data/apps/web/releases"));
        for failure in ["current-link", "metadata-restore"] {
            fs::write(
                worker.path.join(format!("fail-{failure}")),
                "fail next call",
            )
            .unwrap();
        }

        let output = upload(&worker, app_type);
        assert!(!output.status.success(), "{app_type}: {output:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("for manual recovery"),
            "{app_type}: {output:?}"
        );
        for failure in ["current-link", "metadata-restore"] {
            assert!(!worker.path.join(format!("fail-{failure}")).exists());
        }
        let release = worker.metadata("current");
        assert_ne!(release, CURRENT);
        assert!(
            worker
                .path
                .join(format!("data/apps/web/releases/{release}"))
                .is_dir()
        );
        assert_eq!(
            entries(&worker.path.join("data/apps/web/releases")).len(),
            original_releases.len() + 1
        );
        if app_type == "process" {
            assert!(
                worker
                    .active_services()
                    .contains(&format!("tamaya-web-{release}.service"))
            );
        }
    }
}

#[test]
fn successful_deploy_and_publish_commit_new_links_and_release() {
    for app_type in ["process", "published"] {
        let worker = WorkerFixture::new(app_type);
        prepare_upload(&worker, app_type);
        let output = upload(&worker, app_type);
        assert!(output.status.success(), "{app_type}: {output:?}");

        let release = worker.metadata("current");
        assert_ne!(release, CURRENT);
        assert_eq!(worker.metadata("previous"), CURRENT);
        assert_eq!(
            worker.link("current"),
            PathBuf::from(format!("releases/{release}"))
        );
        assert_eq!(
            worker.link("previous"),
            PathBuf::from(format!("releases/{CURRENT}"))
        );
        assert!(
            worker
                .path
                .join(format!("data/apps/web/releases/{release}"))
                .is_dir()
        );
        let route = fs::read_to_string(worker.path.join("caddy/conf.d/example.com.caddy")).unwrap();
        if app_type == "process" {
            assert!(route.contains("reverse_proxy 127.0.0.1:20001"));
            assert_eq!(
                worker.active_services(),
                vec![format!("tamaya-web-{release}.service")]
            );
        } else {
            assert!(route.contains(&format!("/releases/{release}/site")));
            assert!(worker.active_services().is_empty());
        }
    }
}

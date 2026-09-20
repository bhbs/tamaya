mod support;

use std::fs;
use std::path::PathBuf;
use support::{CURRENT, PREVIOUS, WorkerFixture};

#[test]
fn stopped_apps_cannot_be_revived_by_live_or_maintenance() {
    for app_type in ["process", "published"] {
        let worker = WorkerFixture::new(app_type);
        worker.success(&["stop", "web"]);
        worker.success(&["maintenance", "--domain", "example.com"]);
        let stopped = worker.snapshot();

        for args in [["live", "web"], ["maintenance", "web"]] {
            let output = worker.run(&args);
            assert!(!output.status.success(), "{app_type} {args:?}: {output:?}");
            assert_eq!(worker.snapshot(), stopped, "{app_type} {args:?}");
        }

        worker.success(&["live", "--domain", "example.com"]);
        assert_eq!(worker.metadata("status"), "stopped");
        assert!(!worker.path.join("data/caddy-routes/web.caddy").exists());
        assert!(!worker.path.join("caddy/conf.d/example.com.caddy").exists());
        assert!(worker.active_services().is_empty());
    }
}

#[test]
fn maintenance_to_live_preserves_the_running_release() {
    for app_type in ["process", "published"] {
        let worker = WorkerFixture::new(app_type);
        let running = worker.snapshot();
        worker.success(&["maintenance", "web"]);
        assert_eq!(worker.metadata("status"), "maintenance");
        worker.success(&["live", "web"]);
        assert_eq!(worker.snapshot(), running, "{app_type}");
    }
}

#[test]
fn live_rejects_an_inactive_service_before_removing_maintenance() {
    let worker = WorkerFixture::new("process");
    worker.success(&["maintenance", "web"]);
    fs::remove_file(
        worker
            .path
            .join(format!("active/tamaya-web-{CURRENT}.service")),
    )
    .unwrap();
    let before = worker.snapshot();

    let output = worker.run(&["live", "web"]);
    assert!(!output.status.success(), "{output:?}");
    assert_eq!(worker.snapshot(), before);
}

#[test]
fn stop_then_rollback_restores_the_previous_release_and_public_route() {
    for app_type in ["process", "published"] {
        let worker = WorkerFixture::new(app_type);
        worker.success(&["stop", "web"]);
        assert_eq!(worker.metadata("status"), "stopped");
        assert!(worker.active_services().is_empty());
        assert!(!worker.path.join("caddy/conf.d/example.com.caddy").exists());

        worker.success(&["rollback", "web"]);
        assert_eq!(worker.metadata("status"), "running");
        assert_eq!(worker.metadata("current"), PREVIOUS);
        assert_eq!(worker.metadata("previous"), CURRENT);
        assert_eq!(
            worker.link("current"),
            PathBuf::from(format!("releases/{PREVIOUS}"))
        );
        assert_eq!(
            worker.link("previous"),
            PathBuf::from(format!("releases/{CURRENT}"))
        );
        let merged = fs::read_to_string(worker.path.join("caddy/conf.d/example.com.caddy"))
            .expect("rollback must restore the merged, publicly served Caddy route");
        if app_type == "process" {
            assert_eq!(worker.metadata("port"), "20001");
            assert!(merged.contains("reverse_proxy 127.0.0.1:20001"), "{merged}");
            assert!(!merged.contains("127.0.0.1:20000"), "{merged}");
            assert_eq!(
                worker.active_services(),
                vec![format!("tamaya-web-{PREVIOUS}.service")]
            );
        } else {
            assert!(
                merged.contains(&format!("/releases/{PREVIOUS}/site")),
                "{merged}"
            );
            assert!(
                !merged.contains(&format!("/releases/{CURRENT}/site")),
                "{merged}"
            );
            assert!(worker.active_services().is_empty());
        }
    }
}

#[test]
fn failed_rollback_restores_metadata_routes_links_and_services() {
    for app_type in ["process", "published"] {
        for stopped in [false, true] {
            for failure in ["validate", "reload", "previous-link"] {
                let worker = WorkerFixture::new(app_type);
                if stopped {
                    worker.success(&["stop", "web"]);
                }
                let before = worker.snapshot();
                let failure_flag = worker.path.join(format!("fail-{failure}"));
                fs::write(&failure_flag, "fail next call").unwrap();
                let output = worker.run(&["rollback", "web"]);
                assert!(
                    !output.status.success(),
                    "{app_type} stopped={stopped} {failure}: {output:?}"
                );
                assert!(!failure_flag.exists(), "failure injection was not reached");
                assert_eq!(
                    worker.snapshot(),
                    before,
                    "{app_type} stopped={stopped} {failure}"
                );
                if app_type == "process" {
                    let log = fs::read_to_string(worker.path.join("systemctl.log")).unwrap();
                    assert!(
                        log.contains(&format!("enable --now tamaya-web-{PREVIOUS}.service")),
                        "{log}"
                    );
                    assert!(
                        log.contains(&format!("disable --now tamaya-web-{PREVIOUS}.service")),
                        "{log}"
                    );
                }
            }
        }
    }
}

#[test]
fn failed_rollback_recovery_keeps_the_release_that_caddy_may_still_serve() {
    let worker = WorkerFixture::new("process");
    for failure in ["current-link", "metadata-restore"] {
        fs::write(
            worker.path.join(format!("fail-{failure}")),
            "fail next call",
        )
        .unwrap();
    }

    let output = worker.run(&["rollback", "web"]);
    assert!(!output.status.success(), "{output:?}");
    for failure in ["current-link", "metadata-restore"] {
        assert!(
            !worker.path.join(format!("fail-{failure}")).exists(),
            "{failure} injection was not reached"
        );
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("manual recovery"), "{stderr}");
    assert_eq!(worker.metadata("current"), PREVIOUS);
    assert!(worker.path.join("data/apps/web/metadata.toml.bak").exists());
    let merged = fs::read_to_string(worker.path.join("caddy/conf.d/example.com.caddy")).unwrap();
    assert!(merged.contains("reverse_proxy 127.0.0.1:20001"), "{merged}");
    assert_eq!(
        worker.active_services(),
        vec![
            format!("tamaya-web-{PREVIOUS}.service"),
            format!("tamaya-web-{CURRENT}.service"),
        ]
    );
    let log = fs::read_to_string(worker.path.join("systemctl.log")).unwrap();
    assert!(
        !log.contains(&format!("disable --now tamaya-web-{PREVIOUS}.service")),
        "{log}"
    );
}

#[test]
fn failed_rollback_restores_legacy_standalone_caddy_routes() {
    for app_type in ["process", "published"] {
        for failure in ["validate", "reload", "current-link"] {
            let worker = WorkerFixture::new(app_type);
            let merged_path = worker.path.join("caddy/conf.d/example.com.caddy");
            let legacy_path = worker.path.join("caddy/conf.d/web.caddy");
            let original_route = fs::read_to_string(&merged_path).unwrap();
            fs::rename(merged_path, &legacy_path).unwrap();
            fs::remove_file(worker.path.join("data/caddy-routes/web.caddy")).unwrap();
            let before = worker.snapshot();
            let failure_flag = worker.path.join(format!("fail-{failure}"));
            fs::write(&failure_flag, "fail next call").unwrap();

            let output = worker.run(&["rollback", "web"]);
            assert!(!output.status.success(), "{app_type} {failure}: {output:?}");
            assert!(!failure_flag.exists(), "failure injection was not reached");
            assert_eq!(worker.snapshot(), before, "{app_type} {failure}");
            assert_eq!(fs::read_to_string(legacy_path).unwrap(), original_route);
        }
    }
}

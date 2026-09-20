mod support;

use std::fs;

use support::WorkerFixture;

#[test]
fn failed_app_live_restores_metadata_routes_and_maintenance_page() {
    for app_type in ["process", "published"] {
        for domain_maintenance in [false, true] {
            for failure in ["validate", "reload"] {
                let worker = WorkerFixture::new(app_type);
                let selector = if domain_maintenance {
                    vec!["maintenance", "--domain", "example.com"]
                } else {
                    vec!["maintenance", "web"]
                };
                worker.success(&selector);
                let before = worker.snapshot();
                let failure_flag = worker.path.join(format!("fail-{failure}"));
                fs::write(&failure_flag, "fail next call").unwrap();

                let output = worker.run(&["live", "web"]);
                assert!(
                    !output.status.success(),
                    "{app_type} domain_maintenance={domain_maintenance} {failure}: {output:?}"
                );
                assert!(!failure_flag.exists(), "failure injection was not reached");
                assert_eq!(worker.snapshot(), before);
                worker.success(&["live", "web"]);
                assert_eq!(worker.metadata("status"), "running");
            }
        }
    }
}

#[test]
fn failed_domain_live_preserves_the_maintenance_page_and_app_state() {
    for app_type in ["process", "published"] {
        for failure in ["validate", "reload"] {
            let worker = WorkerFixture::new(app_type);
            worker.success(&["maintenance", "--domain", "example.com"]);
            let before = worker.snapshot();
            let failure_flag = worker.path.join(format!("fail-{failure}"));
            fs::write(&failure_flag, "fail next call").unwrap();

            let output = worker.run(&["live", "--domain", "example.com"]);
            assert!(!output.status.success(), "{app_type} {failure}: {output:?}");
            assert!(!failure_flag.exists(), "failure injection was not reached");
            assert_eq!(worker.snapshot(), before);
            worker.success(&["live", "--domain", "example.com"]);
            assert_eq!(worker.metadata("status"), "running");
        }
    }
}

#[test]
fn failed_maintenance_changes_restore_the_previous_message_and_state() {
    for app_type in ["process", "published"] {
        for domain_maintenance in [false, true] {
            for already_in_maintenance in [false, true] {
                for failure in ["validate", "reload"] {
                    let worker = WorkerFixture::new(app_type);
                    let mut args = if domain_maintenance {
                        vec!["maintenance", "--domain", "example.com"]
                    } else {
                        vec!["maintenance", "web"]
                    };
                    args.extend(["--message", "Original maintenance message"]);
                    if already_in_maintenance {
                        worker.success(&args);
                    }
                    let before = worker.snapshot();
                    let failure_flag = worker.path.join(format!("fail-{failure}"));
                    fs::write(&failure_flag, "fail next call").unwrap();
                    *args.last_mut().unwrap() = "Replacement maintenance message";

                    let output = worker.run(&args);
                    assert!(
                        !output.status.success(),
                        "{app_type} domain={domain_maintenance} existing={already_in_maintenance} {failure}: {output:?}"
                    );
                    assert!(!failure_flag.exists(), "failure injection was not reached");
                    assert_eq!(worker.snapshot(), before);
                }
            }
        }
    }
}

#[test]
fn successful_maintenance_updates_replace_the_message_and_live_removes_the_page() {
    for app_type in ["process", "published"] {
        for domain_maintenance in [false, true] {
            let worker = WorkerFixture::new(app_type);
            let running = worker.snapshot();
            let mut args = if domain_maintenance {
                vec!["maintenance", "--domain", "example.com"]
            } else {
                vec!["maintenance", "web"]
            };
            args.extend(["--message", "Original maintenance message"]);
            worker.success(&args);
            *args.last_mut().unwrap() = "Replacement maintenance message";
            worker.success(&args);
            let page = fs::read_to_string(
                worker
                    .path
                    .join("data/static/maintenance/example.com/index.html"),
            )
            .unwrap();
            assert!(page.contains("Replacement maintenance message"));
            assert!(!page.contains("Original maintenance message"));
            args.truncate(args.len() - 2);
            args[0] = "live";
            worker.success(&args);
            assert_eq!(worker.snapshot(), running);
        }
    }
}

mod support;

use std::fs;
use std::path::{Path, PathBuf};
use support::{CURRENT, WorkerFixture};

fn prepare_worker(domain: &str, path: &str, kind: &str) -> WorkerFixture {
    let worker = WorkerFixture::new("process");
    fs::write(worker.path.join("upload"), "fixture binary").unwrap();
    let metadata_path = worker.path.join("data/apps/web/metadata.toml");
    let mut metadata: toml::Table = fs::read_to_string(&metadata_path).unwrap().parse().unwrap();
    for (key, value) in [("domain", domain), ("path", path), ("route_kind", kind)] {
        metadata.insert(key.to_owned(), toml::Value::String(value.to_owned()));
    }
    fs::write(metadata_path, toml::to_string(&metadata).unwrap()).unwrap();
    let snippet = worker.path.join("data/caddy-routes/web.caddy");
    let merged = worker.path.join("caddy/conf.d/example.com.caddy");
    if kind == "none" {
        fs::remove_file(snippet).unwrap();
        fs::remove_file(merged).unwrap();
    } else if kind == "root" {
        fs::write(&snippet, "handle {\n    reverse_proxy 127.0.0.1:20000\n}\n").unwrap();
        fs::write(
            merged,
            "example.com {\n    handle {\n        reverse_proxy 127.0.0.1:20000\n    }\n}\n",
        )
        .unwrap();
    }
    worker
}

fn assert_deployed_route(worker: &WorkerFixture, domain: &str, path: &str, kind: &str) {
    assert_eq!(worker.metadata("domain"), domain);
    assert_eq!(worker.metadata("path"), path);
    assert_eq!(worker.metadata("route_kind"), kind);
    assert_eq!(worker.metadata("status"), "running");
    let release = worker.metadata("current");
    assert_ne!(release, CURRENT);
    assert_eq!(worker.metadata("previous"), CURRENT);
    assert_eq!(
        worker.link("current"),
        PathBuf::from(format!("releases/{release}"))
    );
    assert_eq!(
        worker.active_services(),
        vec![format!("tamaya-web-{release}.service")]
    );
    let snippet = worker.path.join("data/caddy-routes/web.caddy");
    let merged = worker.path.join("caddy/conf.d/example.com.caddy");
    if kind == "none" {
        assert!(!snippet.exists());
        assert!(!merged.exists());
    } else {
        let snippet = fs::read_to_string(snippet).unwrap();
        let merged = fs::read_to_string(merged).unwrap();
        let proxy = format!("reverse_proxy 127.0.0.1:{}", worker.metadata("port"));
        assert!(snippet.contains(&proxy), "{snippet}");
        assert!(merged.contains(&proxy), "{merged}");
        assert!(!merged.contains("127.0.0.1:20000"), "{merged}");
        if kind == "path" {
            let matcher = format!("path {path} {path}/*");
            assert!(snippet.contains(&matcher), "{snippet}");
            assert!(merged.contains(&matcher), "{merged}");
        } else {
            assert!(!snippet.contains("@tamaya_web"), "{snippet}");
            assert!(snippet.contains("handle {"), "{snippet}");
        }
    }
}

fn add_other_app(worker: &WorkerFixture, domain: &str, path: &str) {
    let source = worker.path.join("data/apps/web/metadata.toml");
    let mut metadata: toml::Table = fs::read_to_string(source).unwrap().parse().unwrap();
    let kind = if path == "/" { "root" } else { "path" };
    for (key, value) in [
        ("app", "zother"),
        ("domain", domain),
        ("path", path),
        ("route_kind", kind),
        ("status", "stopped"),
        ("unit", "tamaya-zother-20260920000000.service"),
    ] {
        metadata.insert(key.to_owned(), toml::Value::String(value.to_owned()));
    }
    fs::create_dir_all(worker.path.join("data/apps/zother")).unwrap();
    fs::write(
        worker.path.join("data/apps/zother/metadata.toml"),
        toml::to_string(&metadata).unwrap(),
    )
    .unwrap();
}

fn entries(path: &Path) -> Vec<PathBuf> {
    let mut entries: Vec<_> = fs::read_dir(path)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    entries.sort();
    entries
}

#[test]
fn deploy_inherits_existing_root_path_and_private_routes() {
    for (domain, path, kind) in [
        ("example.com", "/", "root"),
        ("example.com", "/web", "path"),
        ("", "", "none"),
    ] {
        let worker = prepare_worker(domain, path, kind);
        worker.success(&["deploy", "web", "--binary", "upload"]);
        assert_deployed_route(&worker, domain, path, kind);
    }
}

#[test]
fn deploy_restores_stopped_apps_using_their_existing_routes() {
    for (domain, path, kind) in [
        ("example.com", "/", "root"),
        ("example.com", "/web", "path"),
        ("", "", "none"),
    ] {
        let worker = prepare_worker(domain, path, kind);
        worker.success(&["stop", "web"]);
        worker.success(&["deploy", "web", "--binary", "upload"]);
        assert_deployed_route(&worker, domain, path, kind);
    }
}

#[test]
fn compatibility_checks_do_not_replace_the_inherited_route_with_another_apps_route() {
    for other_domain in ["example.com", "other.example.com"] {
        let worker = prepare_worker("example.com", "/web", "path");
        add_other_app(&worker, other_domain, "/other");
        let other_metadata = worker.path.join("data/apps/zother/metadata.toml");
        let original_other_metadata = fs::read_to_string(&other_metadata).unwrap();

        worker.success(&["deploy", "web", "--binary", "upload"]);

        assert_deployed_route(&worker, "example.com", "/web", "path");
        assert_eq!(
            fs::read_to_string(other_metadata).unwrap(),
            original_other_metadata
        );
    }
}

#[test]
fn conflicting_inherited_routes_are_rejected_before_upload_or_service_changes() {
    for (path, kind) in [("/", "root"), ("/web", "path")] {
        let worker = prepare_worker("example.com", path, kind);
        add_other_app(&worker, "example.com", path);
        let before = worker.snapshot();
        let releases = entries(&worker.path.join("data/apps/web/releases"));
        let units = entries(&worker.path.join("systemd"));

        let output = worker.run(&["deploy", "web", "--binary", "upload"]);

        assert!(!output.status.success(), "{kind}: {output:?}");
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(
            error.contains("already has") && error.contains("zother"),
            "{error}"
        );
        assert!(!error.contains("uploading release binary"), "{error}");
        assert!(!worker.path.join("systemctl.log").exists());
        assert_eq!(worker.snapshot(), before);
        assert_eq!(
            entries(&worker.path.join("data/apps/web/releases")),
            releases
        );
        assert_eq!(entries(&worker.path.join("systemd")), units);
    }
}

#[test]
fn explicitly_requested_routes_override_existing_routes() {
    let worker = prepare_worker("example.com", "/web", "path");
    worker.success(&[
        "deploy",
        "web",
        "--binary",
        "upload",
        "--domain",
        "example.com",
        "--path",
        "/new",
    ]);
    assert_deployed_route(&worker, "example.com", "/new", "path");
}

#[test]
fn a_first_deploy_without_a_domain_creates_a_private_route() {
    let worker = prepare_worker("example.com", "/web", "path");
    for directory in ["data/apps/web", "systemd", "active"] {
        fs::remove_dir_all(worker.path.join(directory)).unwrap();
        fs::create_dir_all(worker.path.join(directory)).unwrap();
    }
    fs::remove_file(worker.path.join("data/caddy-routes/web.caddy")).unwrap();
    fs::remove_file(worker.path.join("caddy/conf.d/example.com.caddy")).unwrap();

    worker.success(&["deploy", "web", "--binary", "upload"]);

    assert_eq!(worker.metadata("domain"), "");
    assert_eq!(worker.metadata("path"), "");
    assert_eq!(worker.metadata("route_kind"), "none");
    assert_eq!(worker.metadata("status"), "running");
    assert_eq!(worker.metadata("previous"), "");
    let release = worker.metadata("current");
    assert_eq!(
        worker.link("current"),
        PathBuf::from(format!("releases/{release}"))
    );
    assert_eq!(
        worker.active_services(),
        vec![format!("tamaya-web-{release}.service")]
    );
    assert!(!worker.path.join("data/apps/web/previous").exists());
    assert!(!worker.path.join("data/caddy-routes/web.caddy").exists());
    assert!(!worker.path.join("caddy/conf.d/example.com.caddy").exists());
}

#[test]
fn an_explicit_domain_without_a_path_selects_the_root_route() {
    let worker = prepare_worker("example.com", "/web", "path");
    worker.success(&[
        "deploy",
        "web",
        "--binary",
        "upload",
        "--domain",
        "example.com",
    ]);
    assert_deployed_route(&worker, "example.com", "/", "root");
}

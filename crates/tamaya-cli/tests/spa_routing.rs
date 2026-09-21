mod support;

use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use support::WorkerFixture;

#[test]
#[ignore = "requires Caddy; set TAMAYA_TEST_CADDY to its executable path"]
fn spa_routes_serve_the_correct_entrypoint_after_publish_rollback_and_live() {
    for prefix in ["/docs", "/nested/app", "/"] {
        let worker = WorkerFixture::new("published");
        fs::write(
            worker.path.join(".tamaya.toml"),
            format!("worker = \"local-fixture\"\ndomain = \"example.com\"\npath = \"{prefix}\"\npublish_type = \"spa\"\n"),
        )
        .unwrap();
        let served = worker
            .path
            .join("upload")
            .join(prefix.trim_start_matches('/'));
        fs::create_dir_all(served.join("assets")).unwrap();
        fs::write(
            worker.path.join("upload/index.html"),
            "wrong root entrypoint",
        )
        .unwrap();
        fs::write(served.join("index.html"), "SPA version one").unwrap();
        fs::write(served.join("assets/app.js"), "console.log('asset');").unwrap();
        worker.success(&["publish", "web", "--static-root", "upload"]);
        assert_served(&worker, prefix, "SPA version one");

        fs::write(served.join("index.html"), "SPA version two").unwrap();
        worker.success(&["publish", "web", "--static-root", "upload"]);
        assert_served(&worker, prefix, "SPA version two");
        worker.success(&["rollback", "web"]);
        assert_served(&worker, prefix, "SPA version one");
        worker.success(&["maintenance", "web"]);
        worker.success(&["live", "web"]);
        assert_served(&worker, prefix, "SPA version one");
    }
}

fn assert_served(worker: &WorkerFixture, prefix: &str, entrypoint: &str) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let merged = fs::read_to_string(worker.path.join("caddy/conf.d/example.com.caddy")).unwrap();
    let config = format!(
        "{{\n admin off\n auto_https off\n}}\n{}",
        merged.replacen("example.com {", &format!("http://{address} {{"), 1)
    );
    let config_path = worker.path.join("Caddyfile.test");
    fs::write(&config_path, config).unwrap();
    let caddy = std::env::var_os("TAMAYA_TEST_CADDY").unwrap_or_else(|| "caddy".into());
    let validation = Command::new(&caddy)
        .args(["validate", "--adapter", "caddyfile", "--config"])
        .arg(&config_path)
        .output()
        .expect("install Caddy or set TAMAYA_TEST_CADDY");
    assert!(validation.status.success(), "{validation:?}");
    let log = worker.path.join("caddy-test.log");
    drop(listener);
    let mut server = Server(
        Command::new(caddy)
            .args(["run", "--adapter", "caddyfile", "--config"])
            .arg(config_path)
            .env("XDG_DATA_HOME", worker.path.join("caddy-data"))
            .env("XDG_CONFIG_HOME", worker.path.join("caddy-config"))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(fs::File::create(&log).unwrap())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if TcpStream::connect(address).is_ok() {
            break;
        }
        assert!(
            server.0.try_wait().unwrap().is_none() && Instant::now() < deadline,
            "Caddy did not start: {}",
            fs::read_to_string(&log).unwrap()
        );
        thread::sleep(Duration::from_millis(20));
    }
    let base = prefix.trim_end_matches('/');
    let mut paths = vec![
        format!("{base}/"),
        format!("{base}/deep/client/route"),
        format!("{base}/deep/client/route?tab=details"),
    ];
    if prefix != "/" {
        paths.push(prefix.to_owned());
    }
    for path in paths {
        let (status, body) = request(address, &path);
        assert_eq!(status, 200, "{prefix} {path}: {body}");
        assert_eq!(body, entrypoint, "{prefix} {path}");
    }
    let (status, body) = request(address, &format!("{base}/assets/app.js"));
    assert_eq!(status, 200);
    assert_eq!(body, "console.log('asset');");
    if prefix != "/" {
        let (_, body) = request(address, &format!("{prefix}-other/deep"));
        // With no fallback handler, Caddy returns an empty response for paths
        // outside this app. They must not be rewritten to its SPA entrypoint.
        assert!(
            body.is_empty(),
            "sibling path entered the SPA route: {body}"
        );
    }
}

fn request(address: std::net::SocketAddr, path: &str) -> (u16, String) {
    let mut stream = TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    write!(
        stream,
        "GET {path} HTTP/1.0\r\nHost: {address}\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    let (headers, body) = response.split_once("\r\n\r\n").unwrap();
    let status = headers.split_whitespace().nth(1).unwrap().parse().unwrap();
    (status, body.to_owned())
}

struct Server(Child);

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

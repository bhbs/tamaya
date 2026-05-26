use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const MAX_HTTP_BODY_BYTES: usize = 16 * 1024;
const MAX_PATH_BYTES: usize = 4096;
const MAX_ID_BYTES: usize = 64;
const MAX_BOOT_ARGS_BYTES: usize = 4096;
const API_HOST: &str = "localhost";
#[allow(dead_code)]
const FIRECRACKER_SOCKET_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MachineConfig {
    pub vcpu_count: u8,
    pub mem_size_mib: u32,
    pub ht_enabled: bool,
}

impl MachineConfig {
    pub fn new(vcpu_count: u8, mem_size_mib: u32) -> Result<Self> {
        if vcpu_count == 0 {
            bail!("vcpu_count must be greater than zero");
        }

        if mem_size_mib == 0 {
            bail!("mem_size_mib must be greater than zero");
        }

        Ok(Self {
            vcpu_count,
            mem_size_mib,
            ht_enabled: false,
        })
    }

    fn to_json(&self) -> String {
        format!(
            r#"{{"vcpu_count":{},"mem_size_mib":{},"smt":{}}}"#,
            self.vcpu_count, self.mem_size_mib, self.ht_enabled
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BootSource {
    pub kernel_image_path: PathBuf,
    pub boot_args: String,
}

impl BootSource {
    pub fn new(
        kernel_image_path: impl Into<PathBuf>,
        boot_args: impl Into<String>,
    ) -> Result<Self> {
        let source = Self {
            kernel_image_path: kernel_image_path.into(),
            boot_args: boot_args.into(),
        };
        source.validate()?;
        Ok(source)
    }

    fn validate(&self) -> Result<()> {
        validate_path("kernel_image_path", &self.kernel_image_path)?;
        validate_bounded("boot_args", &self.boot_args, MAX_BOOT_ARGS_BYTES)?;
        Ok(())
    }

    fn to_json(&self) -> String {
        format!(
            r#"{{"kernel_image_path":"{}","boot_args":"{}"}}"#,
            json_escape(&path_to_string(&self.kernel_image_path)),
            json_escape(&self.boot_args)
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Drive {
    pub drive_id: String,
    pub path_on_host: PathBuf,
    pub is_root_device: bool,
    pub is_read_only: bool,
}

impl Drive {
    pub fn rootfs(path_on_host: impl Into<PathBuf>, is_read_only: bool) -> Result<Self> {
        let drive = Self {
            drive_id: "rootfs".to_string(),
            path_on_host: path_on_host.into(),
            is_root_device: true,
            is_read_only,
        };
        drive.validate()?;
        Ok(drive)
    }

    fn validate(&self) -> Result<()> {
        validate_bounded("drive_id", &self.drive_id, MAX_ID_BYTES)?;
        validate_path("path_on_host", &self.path_on_host)?;
        Ok(())
    }

    fn to_json(&self) -> String {
        format!(
            r#"{{"drive_id":"{}","path_on_host":"{}","is_root_device":{},"is_read_only":{}}}"#,
            json_escape(&self.drive_id),
            json_escape(&path_to_string(&self.path_on_host)),
            self.is_root_device,
            self.is_read_only
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkInterface {
    pub iface_id: String,
    pub host_dev_name: String,
    pub guest_mac: Option<String>,
}

impl NetworkInterface {
    pub fn new(
        iface_id: impl Into<String>,
        host_dev_name: impl Into<String>,
        guest_mac: Option<String>,
    ) -> Result<Self> {
        let interface = Self {
            iface_id: iface_id.into(),
            host_dev_name: host_dev_name.into(),
            guest_mac,
        };
        interface.validate()?;
        Ok(interface)
    }

    fn validate(&self) -> Result<()> {
        validate_bounded("iface_id", &self.iface_id, MAX_ID_BYTES)?;
        validate_bounded("host_dev_name", &self.host_dev_name, MAX_ID_BYTES)?;

        if let Some(guest_mac) = &self.guest_mac {
            validate_bounded("guest_mac", guest_mac, MAX_ID_BYTES)?;
        }

        Ok(())
    }

    fn to_json(&self) -> String {
        let guest_mac = match &self.guest_mac {
            Some(guest_mac) => format!(r#","guest_mac":"{}""#, json_escape(guest_mac)),
            None => String::new(),
        };

        format!(
            r#"{{"iface_id":"{}","host_dev_name":"{}"{}}}"#,
            json_escape(&self.iface_id),
            json_escape(&self.host_dev_name),
            guest_mac
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BootPlan {
    pub machine_config: MachineConfig,
    pub boot_source: BootSource,
    pub rootfs: Drive,
    pub network_interface: NetworkInterface,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnixHttpRequest {
    pub method: String,
    pub path: String,
    pub body: String,
}

impl UnixHttpRequest {
    pub fn new(method: impl Into<String>, path: impl Into<String>, body: String) -> Result<Self> {
        let request = Self {
            method: method.into(),
            path: path.into(),
            body,
        };

        if request.body.len() > MAX_HTTP_BODY_BYTES {
            bail!("request body exceeds {MAX_HTTP_BODY_BYTES} bytes");
        }

        Ok(request)
    }

    pub fn to_http_payload(&self) -> Vec<u8> {
        format!(
            "{} {} HTTP/1.1\r\nHost: {}\r\nAccept: application/json\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            self.method,
            self.path,
            API_HOST,
            self.body.len(),
            self.body
        )
        .into_bytes()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FirecrackerClient {
    api_socket_path: PathBuf,
}

impl FirecrackerClient {
    pub fn new(api_socket_path: impl Into<PathBuf>) -> Result<Self> {
        let api_socket_path = api_socket_path.into();
        validate_path("api_socket_path", &api_socket_path)?;
        Ok(Self { api_socket_path })
    }

    pub fn api_socket_path(&self) -> &Path {
        &self.api_socket_path
    }

    pub fn put_machine_config(&self, machine_config: &MachineConfig) -> UnixHttpRequest {
        UnixHttpRequest::new("PUT", "/machine-config", machine_config.to_json())
            .expect("machine config request body is bounded")
    }

    pub fn put_boot_source(&self, boot_source: &BootSource) -> Result<UnixHttpRequest> {
        boot_source.validate()?;
        UnixHttpRequest::new("PUT", "/boot-source", boot_source.to_json())
    }

    pub fn put_rootfs_drive(&self, drive: &Drive) -> Result<UnixHttpRequest> {
        drive.validate()?;

        if !drive.is_root_device {
            bail!("rootfs drive must be marked as the root device");
        }

        UnixHttpRequest::new(
            "PUT",
            format!("/drives/{}", drive.drive_id),
            drive.to_json(),
        )
    }

    pub fn put_network_interface(&self, interface: &NetworkInterface) -> Result<UnixHttpRequest> {
        interface.validate()?;
        UnixHttpRequest::new(
            "PUT",
            format!("/network-interfaces/{}", interface.iface_id),
            interface.to_json(),
        )
    }

    pub fn build_boot_requests(&self, plan: &BootPlan) -> Result<Vec<UnixHttpRequest>> {
        Ok(vec![
            self.put_machine_config(&plan.machine_config),
            self.put_boot_source(&plan.boot_source)?,
            self.put_rootfs_drive(&plan.rootfs)?,
            self.put_network_interface(&plan.network_interface)?,
        ])
    }

    pub fn start_instance(&self) -> Result<UnixHttpRequest> {
        UnixHttpRequest::new(
            "PUT",
            "/actions",
            r#"{"action_type":"InstanceStart"}"#.to_string(),
        )
    }

    #[allow(dead_code)]
    pub fn send(&self, request: &UnixHttpRequest) -> Result<()> {
        let mut stream = UnixStream::connect(&self.api_socket_path).context(format!(
            "failed to connect to Firecracker API socket {}",
            self.api_socket_path.display()
        ))?;
        stream
            .set_read_timeout(Some(FIRECRACKER_SOCKET_TIMEOUT))
            .context("failed to set Firecracker API read timeout")?;
        stream
            .write_all(&request.to_http_payload())
            .context(format!(
                "failed to send {} {}",
                request.method, request.path
            ))?;
        stream
            .shutdown(std::net::Shutdown::Write)
            .context("failed to finish Firecracker API request")?;

        let response = read_http_response(&mut stream)
            .context(format!("failed to read response for {}", request.path))?;
        ensure_success_response(&response, request)
    }

    #[allow(dead_code)]
    pub fn boot(&self, plan: &BootPlan) -> Result<()> {
        for request in self.build_boot_requests(plan)? {
            self.send(&request)?;
        }

        self.send(&self.start_instance()?)
    }
}

#[derive(Debug)]
pub struct FirecrackerProcess {
    child: Child,
}

#[allow(dead_code)]
impl FirecrackerProcess {
    pub fn start(
        binary: &Path,
        api_socket_path: &Path,
        log_dir: &Path,
    ) -> Result<FirecrackerProcess> {
        if let Some(parent) = api_socket_path.parent() {
            fs::create_dir_all(parent).context(format!(
                "failed to create Firecracker runtime directory {}",
                parent.display()
            ))?;
        }
        fs::create_dir_all(log_dir).context(format!(
            "failed to create log directory {}",
            log_dir.display()
        ))?;

        remove_stale_socket(api_socket_path)?;

        let stdout = File::create(log_dir.join("firecracker.stdout.log"))
            .context("failed to create Firecracker stdout log")?;
        let stderr = File::create(log_dir.join("firecracker.stderr.log"))
            .context("failed to create Firecracker stderr log")?;
        let child = Command::new(binary)
            .arg("--api-sock")
            .arg(api_socket_path)
            .stdin(Stdio::null())
            .stdout(stdout)
            .stderr(stderr)
            .spawn()
            .context(format!(
                "failed to start Firecracker binary {}",
                binary.display()
            ))?;

        let mut process = FirecrackerProcess { child };
        process.wait_for_api_socket(api_socket_path)?;
        Ok(process)
    }

    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    fn wait_for_api_socket(&mut self, api_socket_path: &Path) -> Result<()> {
        let deadline = Instant::now() + FIRECRACKER_SOCKET_TIMEOUT;

        while Instant::now() < deadline {
            if api_socket_path.exists() {
                return Ok(());
            }

            if let Some(status) = self
                .child
                .try_wait()
                .context("failed to poll Firecracker process")?
            {
                bail!("Firecracker exited before creating API socket: {status}");
            }

            thread::sleep(Duration::from_millis(25));
        }

        bail!(
            "timed out waiting for Firecracker API socket {}",
            api_socket_path.display()
        )
    }
}

fn validate_path(name: &str, path: &Path) -> Result<()> {
    let path = path_to_string(path);

    if path.is_empty() {
        bail!("{name} must not be empty");
    }

    validate_bounded(name, &path, MAX_PATH_BYTES)
}

#[allow(dead_code)]
fn remove_stale_socket(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).context(format!(
            "failed to remove stale Firecracker API socket {}",
            path.display()
        )),
    }
}

#[allow(dead_code)]
fn ensure_success_response(response: &str, request: &UnixHttpRequest) -> Result<()> {
    let status_line = response.lines().next().unwrap_or_default();
    let success = status_line.starts_with("HTTP/1.1 2") || status_line.starts_with("HTTP/1.0 2");

    if success {
        return Ok(());
    }

    bail!(
        "Firecracker API request failed: {} {}: {}",
        request.method,
        request.path,
        if response.is_empty() {
            "<empty response>"
        } else {
            response
        }
    )
}

#[allow(dead_code)]
fn read_http_response(stream: &mut UnixStream) -> Result<String> {
    let mut response = Vec::new();
    let mut buffer = [0; 4096];

    loop {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => {
                response.extend_from_slice(&buffer[..read]);
                if response_has_complete_body(&response) {
                    break;
                }
            }
            Err(error)
                if error.kind() == std::io::ErrorKind::WouldBlock
                    || error.kind() == std::io::ErrorKind::TimedOut =>
            {
                if response.is_empty() {
                    return Err(error).context("timed out before Firecracker API response");
                }
                break;
            }
            Err(error) => return Err(error).context("failed to read Firecracker API response"),
        }
    }

    String::from_utf8(response).context("Firecracker API response is not utf-8")
}

#[allow(dead_code)]
fn response_has_complete_body(response: &[u8]) -> bool {
    let Some(header_end) = find_header_end(response) else {
        return false;
    };
    let Ok(headers) = std::str::from_utf8(&response[..header_end]) else {
        return false;
    };

    let Some(content_length) = headers.lines().find_map(parse_content_length) else {
        return false;
    };

    response.len() >= header_end + 4 + content_length
}

#[allow(dead_code)]
fn find_header_end(response: &[u8]) -> Option<usize> {
    response.windows(4).position(|window| window == b"\r\n\r\n")
}

#[allow(dead_code)]
fn parse_content_length(line: &str) -> Option<usize> {
    let (name, value) = line.split_once(':')?;
    if !name.eq_ignore_ascii_case("content-length") {
        return None;
    }

    value.trim().parse().ok()
}

fn validate_bounded(name: &str, value: &str, max_bytes: usize) -> Result<()> {
    if value.is_empty() {
        bail!("{name} must not be empty");
    }

    if value.len() > max_bytes {
        bail!("{name} exceeds {max_bytes} bytes");
    }

    Ok(())
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn json_escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());

    for ch in value.chars() {
        match ch {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            ch if ch.is_control() => {
                escaped.push_str(&format!("\\u{:04x}", ch as u32));
            }
            ch => escaped.push(ch),
        }
    }

    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan() -> BootPlan {
        BootPlan {
            machine_config: MachineConfig::new(1, 256).unwrap(),
            boot_source: BootSource::new(
                "/var/lib/v/kernel",
                "console=ttyS0 reboot=k panic=1 pci=off",
            )
            .unwrap(),
            rootfs: Drive::rootfs("/var/lib/v/rootfs.ext4", false).unwrap(),
            network_interface: NetworkInterface::new(
                "eth0",
                "tap-v0",
                Some("AA:FC:00:00:00:01".to_string()),
            )
            .unwrap(),
        }
    }

    #[test]
    fn builds_machine_config_request_payload() {
        let client = FirecrackerClient::new("/tmp/firecracker.socket").unwrap();
        let request = client.put_machine_config(&MachineConfig::new(2, 512).unwrap());

        assert_eq!(request.method, "PUT");
        assert_eq!(request.path, "/machine-config");
        assert_eq!(
            request.body,
            r#"{"vcpu_count":2,"mem_size_mib":512,"smt":false}"#
        );

        let payload = String::from_utf8(request.to_http_payload()).unwrap();
        assert!(payload.starts_with("PUT /machine-config HTTP/1.1\r\n"));
        assert!(payload.contains("Content-Type: application/json\r\n"));
        assert!(payload.ends_with(&request.body));
    }

    #[test]
    fn builds_ordered_boot_plan_requests() {
        let client = FirecrackerClient::new("/tmp/firecracker.socket").unwrap();
        let requests = client.build_boot_requests(&plan()).unwrap();
        let paths: Vec<&str> = requests
            .iter()
            .map(|request| request.path.as_str())
            .collect();

        assert_eq!(
            paths,
            vec![
                "/machine-config",
                "/boot-source",
                "/drives/rootfs",
                "/network-interfaces/eth0"
            ]
        );
    }

    #[test]
    fn builds_start_instance_request() {
        let client = FirecrackerClient::new("/tmp/firecracker.socket").unwrap();
        let request = client.start_instance().unwrap();

        assert_eq!(request.method, "PUT");
        assert_eq!(request.path, "/actions");
        assert_eq!(request.body, r#"{"action_type":"InstanceStart"}"#);
    }

    #[test]
    fn detects_complete_http_response_body() {
        assert!(response_has_complete_body(
            b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n"
        ));
        assert!(response_has_complete_body(
            b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\n{}"
        ));
        assert!(!response_has_complete_body(
            b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{"
        ));
        assert!(!response_has_complete_body(b"HTTP/1.1 200 OK\r\n"));
    }

    #[test]
    fn parses_content_length_header() {
        assert_eq!(parse_content_length("Content-Length: 12"), Some(12));
        assert_eq!(parse_content_length("content-length: 0"), Some(0));
        assert_eq!(parse_content_length("Content-Type: application/json"), None);
        assert_eq!(parse_content_length("Content-Length: nope"), None);
    }

    #[test]
    fn escapes_json_strings_without_external_dependencies() {
        let source =
            BootSource::new("/kernels/firecracker", "console=\"ttyS0\"\nreboot=k").unwrap();

        assert_eq!(
            source.to_json(),
            r#"{"kernel_image_path":"/kernels/firecracker","boot_args":"console=\"ttyS0\"\nreboot=k"}"#
        );
    }

    #[test]
    fn rejects_unbounded_or_invalid_values() {
        assert!(MachineConfig::new(0, 128).is_err());
        assert!(MachineConfig::new(1, 0).is_err());
        assert!(FirecrackerClient::new("").is_err());

        let too_long_id = "x".repeat(MAX_ID_BYTES + 1);
        assert!(NetworkInterface::new(too_long_id, "tap0", None).is_err());
    }

    #[test]
    fn rejects_non_root_drive_requests() {
        let mut drive = Drive::rootfs("/var/lib/v/rootfs.ext4", true).unwrap();
        drive.is_root_device = false;

        let client = FirecrackerClient::new("/tmp/firecracker.socket").unwrap();
        assert!(client.put_rootfs_drive(&drive).is_err());
    }

    #[test]
    fn rejects_empty_and_oversized_request_values() {
        assert!(BootSource::new("", "console=ttyS0").is_err());
        assert!(BootSource::new("/kernel", "x".repeat(MAX_BOOT_ARGS_BYTES + 1)).is_err());
        assert!(Drive::rootfs("", true).is_err());
        assert!(NetworkInterface::new("eth0", "", None).is_err());
        assert!(NetworkInterface::new("eth0", "tap0", Some("x".repeat(MAX_ID_BYTES + 1))).is_err());

        let oversized_body = "x".repeat(MAX_HTTP_BODY_BYTES + 1);
        assert!(UnixHttpRequest::new("PUT", "/oversized", oversized_body).is_err());
    }

    #[test]
    fn client_methods_revalidate_inputs() {
        let client = FirecrackerClient::new("/tmp/firecracker.socket").unwrap();

        let invalid_source = BootSource {
            kernel_image_path: PathBuf::new(),
            boot_args: String::new(),
        };
        assert!(client.put_boot_source(&invalid_source).is_err());

        let invalid_drive = Drive {
            drive_id: String::new(),
            path_on_host: PathBuf::new(),
            is_root_device: true,
            is_read_only: true,
        };
        assert!(client.put_rootfs_drive(&invalid_drive).is_err());

        let invalid_interface = NetworkInterface {
            iface_id: String::new(),
            host_dev_name: String::new(),
            guest_mac: None,
        };
        assert!(client.put_network_interface(&invalid_interface).is_err());

        let invalid_plan = BootPlan {
            machine_config: MachineConfig::new(1, 256).unwrap(),
            boot_source: invalid_source,
            rootfs: Drive::rootfs("/rootfs.ext4", true).unwrap(),
            network_interface: NetworkInterface::new("eth0", "tap0", None).unwrap(),
        };
        assert!(client.build_boot_requests(&invalid_plan).is_err());

        let invalid_drive_plan = BootPlan {
            machine_config: MachineConfig::new(1, 256).unwrap(),
            boot_source: BootSource::new("/kernel", "console=ttyS0").unwrap(),
            rootfs: invalid_drive,
            network_interface: NetworkInterface::new("eth0", "tap0", None).unwrap(),
        };
        assert!(client.build_boot_requests(&invalid_drive_plan).is_err());

        let invalid_interface_plan = BootPlan {
            machine_config: MachineConfig::new(1, 256).unwrap(),
            boot_source: BootSource::new("/kernel", "console=ttyS0").unwrap(),
            rootfs: Drive::rootfs("/rootfs.ext4", true).unwrap(),
            network_interface: invalid_interface,
        };
        assert!(client.build_boot_requests(&invalid_interface_plan).is_err());
    }

    #[test]
    fn escapes_all_json_special_cases() {
        assert_eq!(
            json_escape("\"\\\n\r\t\u{0007}"),
            "\\\"\\\\\\n\\r\\t\\u0007"
        );
    }
}

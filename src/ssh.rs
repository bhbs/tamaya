use crate::config::WorkerConfig;
use crate::firecracker::UnixHttpRequest;
use anyhow::{Context, Result, bail};
use std::ffi::OsString;
use std::fs::File;
use std::io;
use std::path::Path;
use std::process::{Command, Output, Stdio};

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SshRunner {
    worker: WorkerConfig,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RemoteBuildArtifact {
    pub rootfs: std::path::PathBuf,
    pub data: std::path::PathBuf,
    pub config: std::path::PathBuf,
    pub metadata: std::path::PathBuf,
}

impl SshRunner {
    pub fn new(worker: WorkerConfig) -> Self {
        Self { worker }
    }

    pub fn command_args(&self, remote_command: &str) -> Vec<OsString> {
        let mut args = Vec::new();

        if let Some(port) = self.worker.port {
            args.push("-p".into());
            args.push(port.to_string().into());
        }

        if let Some(identity_file) = &self.worker.identity_file {
            args.push("-i".into());
            args.push(identity_file.as_os_str().to_owned());
        }

        args.push(self.worker.ssh_target().into());
        args.push(remote_command.into());
        args
    }

    pub fn command(&self, remote_command: &str) -> Command {
        let mut command = Command::new(ssh_bin());
        command.args(self.command_args(remote_command));
        command
    }

    pub fn shell_command(&self, script: &str) -> Command {
        self.command(&format!("sh -lc {}", shell_quote(script)))
    }

    pub fn run_shell(&self, script: &str) -> Result<Output> {
        let output = self.shell_command(script).output().context(format!(
            "failed to run ssh command on worker {}",
            self.worker.ssh_target()
        ))?;

        if output.status.success() {
            return Ok(output);
        }

        bail!(
            "ssh command failed on worker {} with status {}: {}",
            self.worker.ssh_target(),
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    pub fn check_capabilities(&self) -> Result<()> {
        self.run_shell(&worker_capability_script(&self.worker))
            .context(format!(
                "worker capability check failed on worker {}",
                self.worker.ssh_target()
            ))?;
        Ok(())
    }

    pub fn create_runtime_dirs(&self, app: &str) -> Result<String> {
        validate_remote_name("app", app)?;
        let output = self
            .run_shell(&create_runtime_dirs_script(app))
            .context("failed to create remote XDG runtime directories")?;
        let stdout = String::from_utf8(output.stdout).context("ssh stdout is not utf-8")?;
        let runtime_dir = stdout
            .lines()
            .next()
            .context("remote runtime directory was not reported")?;

        Ok(runtime_dir.to_string())
    }

    pub fn require_readable_file(&self, name: &str, path: &Path) -> Result<String> {
        let path = path_to_remote_string(path)?;
        let output = self
            .run_shell(&require_readable_file_script(name, &path))
            .context(format!("failed to validate remote {name} path {path}"))?;
        let stdout = String::from_utf8(output.stdout).context("ssh stdout is not utf-8")?;
        let resolved = stdout
            .lines()
            .next()
            .context(format!("remote {name} path was not reported"))?;

        Ok(resolved.to_string())
    }

    pub fn upload_boot_file(&self, app: &str, kind: &str, local_path: &Path) -> Result<String> {
        validate_remote_name("app", app)?;
        validate_remote_name("kind", kind)?;
        let filename = local_path
            .file_name()
            .and_then(|value| value.to_str())
            .context("local boot file must have a UTF-8 file name")?;
        validate_remote_name("filename", filename)?;

        let mut local_file = File::open(local_path).context(format!(
            "failed to open local boot file {}",
            local_path.display()
        ))?;
        let mut child = self
            .shell_command(&upload_boot_file_script(app, kind, filename))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("failed to run ssh upload command")?;

        {
            let mut stdin = child.stdin.take().context("failed to open ssh stdin")?;
            io::copy(&mut local_file, &mut stdin).context(format!(
                "failed to stream local boot file {}",
                local_path.display()
            ))?;
        }

        let output = child
            .wait_with_output()
            .context("failed to wait for ssh upload command")?;
        if !output.status.success() {
            bail!(
                "ssh upload failed on worker {} with status {}: {}",
                self.worker.ssh_target(),
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }

        let stdout = String::from_utf8(output.stdout).context("ssh stdout is not utf-8")?;
        let remote_path = stdout
            .lines()
            .next()
            .context("remote boot file path was not reported")?;

        Ok(remote_path.to_string())
    }

    pub fn upload_artifact_tar(&self, app: &str, local_path: &Path) -> Result<String> {
        validate_remote_name("app", app)?;
        let mut local_file = File::open(local_path)
            .context(format!("failed to open artifact {}", local_path.display()))?;
        let mut child = self
            .shell_command(&upload_artifact_tar_script(app))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("failed to run ssh artifact upload command")?;

        {
            let mut stdin = child.stdin.take().context("failed to open ssh stdin")?;
            io::copy(&mut local_file, &mut stdin).context(format!(
                "failed to stream artifact {}",
                local_path.display()
            ))?;
        }

        let output = child
            .wait_with_output()
            .context("failed to wait for ssh artifact upload command")?;
        if !output.status.success() {
            bail!(
                "ssh artifact upload failed on worker {} with status {}: {}",
                self.worker.ssh_target(),
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }

        let stdout = String::from_utf8(output.stdout).context("ssh stdout is not utf-8")?;
        let remote_path = stdout
            .lines()
            .next()
            .context("remote artifact path was not reported")?;

        Ok(remote_path.to_string())
    }

    pub fn materialize_rootfs_from_artifact(
        &self,
        app: &str,
        artifact_path: &Path,
        rootfs_size_mib: u64,
        data_size_mib: u64,
        port: u16,
        init: &str,
    ) -> Result<RemoteBuildArtifact> {
        validate_remote_name("app", app)?;
        validate_remote_path_value("init", init)?;
        let artifact_path = path_to_remote_string(artifact_path)?;
        let output = self
            .run_shell(&materialize_rootfs_from_artifact_script(
                app,
                &artifact_path,
                rootfs_size_mib,
                data_size_mib,
                port,
                init,
            ))
            .context("failed to materialize rootfs on worker")?;
        let stdout = String::from_utf8(output.stdout).context("ssh stdout is not utf-8")?;
        let mut lines = stdout.lines();
        let rootfs = lines
            .next()
            .context("remote rootfs path was not reported")?;
        let data = lines.next().context("remote data path was not reported")?;
        let config = lines
            .next()
            .context("remote app config path was not reported")?;
        let metadata = lines
            .next()
            .context("remote metadata path was not reported")?;

        Ok(RemoteBuildArtifact {
            rootfs: rootfs.into(),
            data: data.into(),
            config: config.into(),
            metadata: metadata.into(),
        })
    }

    pub fn require_tap_interface(&self, tap: &str) -> Result<String> {
        validate_remote_name("tap", tap)?;
        let output = self
            .run_shell(&require_tap_interface_script(tap))
            .context(format!("failed to validate remote TAP interface {tap}"))?;
        let stdout = String::from_utf8(output.stdout).context("ssh stdout is not utf-8")?;
        let resolved = stdout
            .lines()
            .next()
            .context("remote TAP interface was not reported")?;

        Ok(resolved.to_string())
    }

    pub fn ensure_tap_interface(&self, tap: &str) -> Result<String> {
        validate_remote_name("tap", tap)?;
        let output = self
            .run_shell(&ensure_tap_interface_script(tap))
            .context(format!(
                "failed to create or enable remote TAP interface {tap}"
            ))?;
        let stdout = String::from_utf8(output.stdout).context("ssh stdout is not utf-8")?;
        let resolved = stdout
            .lines()
            .next()
            .context("remote TAP interface was not reported")?;

        Ok(resolved.to_string())
    }

    pub fn delete_tap_interface(&self, tap: &str) -> Result<()> {
        validate_remote_name("tap", tap)?;
        self.run_shell(&delete_tap_interface_script(tap))
            .context(format!("failed to delete remote TAP interface {tap}"))?;
        Ok(())
    }

    pub fn cleanup_stale_tap_interfaces(&self, preserve_taps: &[String]) -> Result<Vec<String>> {
        for tap in preserve_taps {
            validate_remote_name("tap", tap)?;
        }
        let output = self
            .run_shell(&cleanup_stale_tap_interfaces_script(preserve_taps))
            .context("failed to clean up stale remote TAP interfaces")?;
        let stdout = String::from_utf8(output.stdout).context("ssh stdout is not utf-8")?;

        Ok(stdout.lines().map(ToOwned::to_owned).collect())
    }

    pub fn start_firecracker(
        &self,
        firecracker_bin: &str,
        api_socket_path: &Path,
        log_dir: &Path,
    ) -> Result<u32> {
        let api_socket_path = path_to_remote_string(api_socket_path)?;
        let log_dir = path_to_remote_string(log_dir)?;
        let output = self
            .run_shell(&start_firecracker_script(
                firecracker_bin,
                &api_socket_path,
                &log_dir,
            ))
            .context("failed to start remote Firecracker")?;
        let stdout = String::from_utf8(output.stdout).context("ssh stdout is not utf-8")?;
        let pid = stdout
            .lines()
            .next()
            .context("remote Firecracker PID was not reported")?
            .parse()
            .context("remote Firecracker PID is not a valid integer")?;

        Ok(pid)
    }

    pub fn send_firecracker_api_requests(
        &self,
        api_socket_path: &Path,
        requests: &[UnixHttpRequest],
    ) -> Result<()> {
        let api_socket_path = path_to_remote_string(api_socket_path)?;
        for request in requests {
            self.run_shell(&firecracker_api_request_script(&api_socket_path, request))
                .context(format!(
                    "failed to send Firecracker API request {} {}",
                    request.method, request.path
                ))?;
        }

        Ok(())
    }

    pub fn stop_firecracker(&self, pid: u32, remote_runtime_dir: &Path) -> Result<()> {
        let remote_runtime_dir = path_to_remote_string(remote_runtime_dir)?;
        self.run_shell(&stop_firecracker_script(pid, &remote_runtime_dir))
            .context("failed to stop remote Firecracker")?;

        Ok(())
    }

    pub fn rename_runtime_dir(&self, old_path: &Path, new_path: &Path) -> Result<()> {
        let old_path = path_to_remote_string(old_path)?;
        let new_path = path_to_remote_string(new_path)?;
        if let Err(error) = self.run_shell(&rename_runtime_dir_script(&old_path, &new_path))
            && self
                .run_shell(&runtime_dir_renamed_script(&old_path, &new_path))
                .is_err()
        {
            return Err(error).context(format!(
                "failed to rename remote runtime dir {old_path} → {new_path}"
            ));
        }
        Ok(())
    }

    pub fn remove_remote_runtime_dir(&self, remote_runtime_dir: &Path) -> Result<()> {
        let remote_runtime_dir = path_to_remote_string(remote_runtime_dir)?;
        self.run_shell(&remove_remote_runtime_dir_script(&remote_runtime_dir))
            .context(format!(
                "failed to remove remote runtime directory {remote_runtime_dir}"
            ))?;
        Ok(())
    }

    pub fn remove_remote_runtime_for_app(&self, app: &str) -> Result<()> {
        validate_remote_name("app", app)?;
        self.run_shell(&remove_remote_runtime_for_app_script(app))
            .context(format!("failed to remove remote runtime for app {app}"))?;
        Ok(())
    }

    pub fn health_check(&self, host: &str, port: u16) -> Result<()> {
        validate_remote_name("health_check_host", host)?;
        self.run_shell(&health_check_script(host, port))
            .context(format!("health check failed for {host}:{port}"))
            .map(|_| ())
    }

    pub fn http_health_check(&self, host: &str, port: u16, path: &str) -> Result<()> {
        validate_remote_name("health_check_host", host)?;
        validate_remote_name("health_check_path", path)?;
        self.run_shell(&http_health_check_script(host, port, path))
            .context(format!(
                "HTTP health check failed for http://{host}:{port}{path}"
            ))
            .map(|_| ())
    }

    pub fn stream_logs(&self, remote_log_dir: &Path) -> Result<()> {
        let remote_log_dir = path_to_remote_string(remote_log_dir)?;
        self.stream_shell(&logs_script(&remote_log_dir))
            .context("failed to stream remote logs")?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn remote_dir_exists(&self, path: &Path) -> Result<bool> {
        let path = path_to_remote_string(path)?;
        let output = self
            .run_shell(&check_remote_dir_exists_script(&path))
            .context(format!("failed to check remote directory {}", path))?;
        let stdout = String::from_utf8(output.stdout).context("ssh stdout is not utf-8")?;
        Ok(stdout.trim() == "exists")
    }

    pub fn stream_shell(&self, script: &str) -> Result<()> {
        let status = self.shell_command(script).status().context(format!(
            "failed to run ssh stream command on worker {}",
            self.worker.ssh_target()
        ))?;
        if !status.success() {
            bail!(
                "ssh stream command failed on worker {} with status {}",
                self.worker.ssh_target(),
                status
            );
        }
        Ok(())
    }

    pub fn update_caddy_config(
        &self,
        app: &str,
        domain: &str,
        vm_host: &str,
        vm_port: u16,
    ) -> Result<()> {
        validate_remote_name("app", app)?;
        let config_dir = path_to_remote_string(&self.worker.caddy_config_dir)?;
        self.run_shell(&update_caddy_config_script(
            app,
            domain,
            vm_host,
            vm_port,
            &config_dir,
        ))
        .context("failed to update Caddy config")?;
        Ok(())
    }

    pub fn remove_caddy_config(&self, app: &str) -> Result<()> {
        validate_remote_name("app", app)?;
        let config_dir = path_to_remote_string(&self.worker.caddy_config_dir)?;
        self.run_shell(&remove_caddy_config_script(app, &config_dir))
            .context("failed to remove Caddy config")?;
        Ok(())
    }

    pub fn reload_caddy(&self) -> Result<()> {
        self.run_shell(&reload_caddy_script())
            .context("failed to reload Caddy")?;
        Ok(())
    }

    pub fn check_caddy(&self) -> Result<()> {
        self.run_shell(&check_caddy_script())
            .context("Caddy is not installed or running on worker")?;
        Ok(())
    }

    pub fn install_worker_prerequisites(&self) -> Result<()> {
        let firecracker_bin = path_to_remote_string(Path::new(&self.worker.firecracker_bin))?;
        let caddy_config_dir = path_to_remote_string(&self.worker.caddy_config_dir)?;
        self.run_shell(&install_worker_prerequisites_script(
            &firecracker_bin,
            &caddy_config_dir,
        ))
        .context("failed to install worker prerequisites")?;
        Ok(())
    }
}

#[cfg(test)]
fn remote_runtime_dir_display(app: &str) -> String {
    format!("${{XDG_RUNTIME_DIR:-${{XDG_STATE_HOME:-$HOME/.local/state}}/v/runtime}}/v/{app}")
}

pub fn validate_remote_name(name: &str, value: &str) -> Result<()> {
    if value.is_empty() {
        bail!("{name} must not be empty");
    }

    if value.len() > 64 {
        bail!("{name} exceeds 64 bytes");
    }

    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        bail!("{name} must contain only ASCII letters, digits, '.', '_', or '-'");
    }

    Ok(())
}

fn shell_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }

    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

macro_rules! ssh_script {
    ($name:literal) => {
        include_str!(concat!("ssh_scripts/", $name))
    };
}

fn render_ssh_script(template: &str, replacements: &[(&str, String)]) -> String {
    let mut script = template.to_string();
    for (key, value) in replacements {
        script = script.replace(&format!("{{{{{key}}}}}"), value);
    }
    script
}

fn worker_capability_script(worker: &WorkerConfig) -> String {
    render_ssh_script(
        ssh_script!("worker_capability.sh"),
        &[(
            "firecracker_bin",
            shell_quote(worker.firecracker_bin.as_str()),
        )],
    )
}

fn start_firecracker_script(firecracker_bin: &str, api_socket_path: &str, log_dir: &str) -> String {
    render_ssh_script(
        ssh_script!("start_firecracker.sh"),
        &[
            ("firecracker_bin", shell_quote(firecracker_bin)),
            ("api_socket_path", shell_quote(api_socket_path)),
            ("log_dir", shell_quote(log_dir)),
        ],
    )
}

fn firecracker_api_request_script(api_socket_path: &str, request: &UnixHttpRequest) -> String {
    render_ssh_script(
        ssh_script!("firecracker_api_request.sh"),
        &[
            ("api_socket_path", shell_quote(api_socket_path)),
            ("method", shell_quote(&request.method)),
            ("body", shell_quote(&request.body)),
            (
                "url",
                shell_quote(&format!("http://localhost{}", request.path)),
            ),
        ],
    )
}

fn stop_firecracker_script(pid: u32, remote_runtime_dir: &str) -> String {
    render_ssh_script(
        ssh_script!("stop_firecracker.sh"),
        &[
            ("pid", pid.to_string()),
            ("remote_runtime_dir", shell_quote(remote_runtime_dir)),
        ],
    )
}

fn rename_runtime_dir_script(old_path: &str, new_path: &str) -> String {
    render_ssh_script(
        ssh_script!("rename_runtime_dir.sh"),
        &[
            ("old_path", shell_quote(old_path)),
            ("new_path", shell_quote(new_path)),
        ],
    )
}

fn runtime_dir_renamed_script(old_path: &str, new_path: &str) -> String {
    render_ssh_script(
        ssh_script!("runtime_dir_renamed.sh"),
        &[
            ("old_path", shell_quote(old_path)),
            ("new_path", shell_quote(new_path)),
        ],
    )
}

fn remove_remote_runtime_dir_script(remote_runtime_dir: &str) -> String {
    render_ssh_script(
        ssh_script!("remove_remote_runtime_dir.sh"),
        &[("remote_runtime_dir", shell_quote(remote_runtime_dir))],
    )
}

fn remove_remote_runtime_for_app_script(app: &str) -> String {
    render_ssh_script(
        ssh_script!("remove_remote_runtime_for_app.sh"),
        &[("app", shell_escape_for_double_quotes(app))],
    )
}

fn require_readable_file_script(name: &str, path: &str) -> String {
    render_ssh_script(
        ssh_script!("require_readable_file.sh"),
        &[("name", shell_quote(name)), ("path", shell_quote(path))],
    )
}

fn upload_boot_file_script(app: &str, kind: &str, filename: &str) -> String {
    render_ssh_script(
        ssh_script!("upload_boot_file.sh"),
        &[
            ("app", shell_escape_for_double_quotes(app)),
            ("kind", shell_escape_for_double_quotes(kind)),
            ("filename", shell_escape_for_double_quotes(filename)),
        ],
    )
}

fn upload_artifact_tar_script(app: &str) -> String {
    render_ssh_script(
        ssh_script!("upload_artifact_tar.sh"),
        &[("app", shell_escape_for_double_quotes(app))],
    )
}

fn materialize_rootfs_from_artifact_script(
    app: &str,
    artifact_path: &str,
    rootfs_size_mib: u64,
    data_size_mib: u64,
    port: u16,
    init: &str,
) -> String {
    render_ssh_script(
        ssh_script!("materialize_rootfs_from_artifact.sh"),
        &[
            ("app", shell_escape_for_double_quotes(app)),
            ("artifact_path", shell_quote(artifact_path)),
            ("rootfs_size_mib", rootfs_size_mib.to_string()),
            ("data_size_mib", data_size_mib.to_string()),
            ("port", port.to_string()),
            ("init", shell_quote(init)),
        ],
    )
}

fn require_tap_interface_script(tap: &str) -> String {
    render_ssh_script(
        ssh_script!("require_tap_interface.sh"),
        &[("tap", shell_quote(tap))],
    )
}

fn ensure_tap_interface_script(tap: &str) -> String {
    render_ssh_script(
        ssh_script!("ensure_tap_interface.sh"),
        &[("tap", shell_quote(tap))],
    )
}

fn delete_tap_interface_script(tap: &str) -> String {
    render_ssh_script(
        ssh_script!("delete_tap_interface.sh"),
        &[("tap", shell_quote(tap))],
    )
}

fn cleanup_stale_tap_interfaces_script(preserve_taps: &[String]) -> String {
    let preserve_pattern = if preserve_taps.is_empty() {
        String::new()
    } else {
        format!(
            r#"
  case "$tap" in
    {}) continue ;;
  esac
"#,
            preserve_taps
                .iter()
                .map(|tap| shell_quote(tap))
                .collect::<Vec<_>>()
                .join("|")
        )
    };

    render_ssh_script(
        ssh_script!("cleanup_stale_tap_interfaces.sh"),
        &[("preserve_pattern", preserve_pattern)],
    )
}

fn create_runtime_dirs_script(app: &str) -> String {
    render_ssh_script(
        ssh_script!("create_runtime_dirs.sh"),
        &[("app", shell_escape_for_double_quotes(app))],
    )
}

fn logs_script(log_dir: &str) -> String {
    render_ssh_script(ssh_script!("logs.sh"), &[("log_dir", shell_quote(log_dir))])
}

fn shell_escape_for_double_quotes(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('$', "\\$")
        .replace('`', "\\`")
}

fn ssh_bin() -> OsString {
    std::env::var_os("V_SSH_BIN").unwrap_or_else(|| OsString::from("ssh"))
}

fn path_to_remote_string(path: &Path) -> Result<String> {
    let value = path
        .to_str()
        .context("remote path must be valid UTF-8")?
        .to_string();

    if value.is_empty() {
        bail!("remote path must not be empty");
    }

    if value.len() > 4096 {
        bail!("remote path exceeds 4096 bytes");
    }

    Ok(value)
}

fn validate_remote_path_value(name: &str, value: &str) -> Result<()> {
    if value.is_empty() {
        bail!("{name} must not be empty");
    }
    if value.len() > 4096 {
        bail!("{name} exceeds 4096 bytes");
    }
    if value.contains('\0') || value.contains('\n') || value.contains('\r') {
        bail!("{name} must not contain control characters");
    }
    Ok(())
}

fn health_check_script(host: &str, port: u16) -> String {
    render_ssh_script(
        ssh_script!("health_check.sh"),
        &[("host", shell_quote(host)), ("port", port.to_string())],
    )
}

fn http_health_check_script(host: &str, port: u16, path: &str) -> String {
    render_ssh_script(
        ssh_script!("http_health_check.sh"),
        &[
            ("host", shell_quote(host)),
            ("port", port.to_string()),
            (
                "path_clean",
                shell_escape_for_double_quotes(path.trim_start_matches('/')),
            ),
        ],
    )
}

fn update_caddy_config_script(
    app: &str,
    domain: &str,
    vm_host: &str,
    vm_port: u16,
    config_dir: &str,
) -> String {
    render_ssh_script(
        ssh_script!("update_caddy_config.sh"),
        &[
            ("app", shell_escape_for_double_quotes(app)),
            ("domain", shell_escape_for_double_quotes(domain)),
            ("domain_block", shell_escape_for_double_quotes(domain)),
            (
                "target",
                shell_escape_for_double_quotes(&format!("{vm_host}:{vm_port}")),
            ),
            ("config_dir", shell_escape_for_double_quotes(config_dir)),
            ("vm_host", shell_escape_for_double_quotes(vm_host)),
            ("vm_port", vm_port.to_string()),
        ],
    )
}

fn remove_caddy_config_script(app: &str, config_dir: &str) -> String {
    render_ssh_script(
        ssh_script!("remove_caddy_config.sh"),
        &[
            ("app", shell_escape_for_double_quotes(app)),
            ("config_dir", shell_escape_for_double_quotes(config_dir)),
        ],
    )
}

fn reload_caddy_script() -> String {
    ssh_script!("reload_caddy.sh").to_string()
}

fn check_caddy_script() -> String {
    ssh_script!("check_caddy.sh").to_string()
}

#[allow(dead_code)]
fn check_remote_dir_exists_script(path: &str) -> String {
    render_ssh_script(
        ssh_script!("check_remote_dir_exists.sh"),
        &[("path", shell_quote(path))],
    )
}

fn install_worker_prerequisites_script(firecracker_bin: &str, caddy_config_dir: &str) -> String {
    render_ssh_script(
        ssh_script!("install_worker_prerequisites.sh"),
        &[
            ("firecracker_bin", shell_quote(firecracker_bin)),
            ("caddy_config_dir", shell_quote(caddy_config_dir)),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn worker() -> WorkerConfig {
        WorkerConfig {
            host: "203.0.113.10".to_string(),
            user: Some("deploy".to_string()),
            port: Some(2222),
            identity_file: Some(PathBuf::from("/keys/prod")),
            firecracker_bin: "/usr/local/bin/firecracker".to_string(),
            caddy_config_dir: PathBuf::from("/etc/caddy/conf.d"),
        }
    }

    #[test]
    fn builds_ssh_arguments() {
        let runner = SshRunner::new(worker());
        let args = runner.command_args("uname -a");

        assert_eq!(
            args,
            vec![
                OsString::from("-p"),
                OsString::from("2222"),
                OsString::from("-i"),
                OsString::from("/keys/prod"),
                OsString::from("deploy@203.0.113.10"),
                OsString::from("uname -a"),
            ]
        );
    }

    #[test]
    fn builds_remote_shell_command() {
        let runner = SshRunner::new(worker());
        let args = runner.command_args("sh -lc 'printf '\"'\"'hello'\"'\"''");

        assert_eq!(args.last().unwrap(), "sh -lc 'printf '\"'\"'hello'\"'\"''");
    }

    #[test]
    fn builds_worker_capability_script() {
        let script = worker_capability_script(&worker());

        assert!(script.contains(r#"[ "$(uname -s)" = "Linux" ]"#));
        assert!(script.contains("[ -e /dev/kvm ]"));
        assert!(script.contains("command -v ip >/dev/null"));
        assert!(script.contains("command -v curl >/dev/null"));
        assert!(script.contains("[ -x '/usr/local/bin/firecracker' ]"));
    }

    #[test]
    fn validates_remote_names() {
        assert!(validate_remote_name("app", "web-1.prod").is_ok());
        assert!(validate_remote_name("app", "").is_err());
        assert!(validate_remote_name("app", "../web").is_err());
        assert!(validate_remote_name("app", "web/api").is_err());
    }

    #[test]
    fn builds_remote_file_validation_script() {
        let script = require_readable_file_script("rootfs", "$XDG_DATA_HOME/v/images/web.ext4");

        assert!(script.contains("XDG_DATA_HOME"));
        assert!(script.contains("[ -f \"$resolved\" ]"));
        assert!(script.contains("rootfs"));
    }

    #[test]
    fn materialize_rootfs_script_keeps_stdout_for_paths_only() {
        let script = materialize_rootfs_from_artifact_script(
            "web",
            "/tmp/artifact.tar",
            1024,
            256,
            3000,
            "/sbin/init",
        );

        assert!(script.contains("tar -C \"$work_dir\" -xf \"$artifact\" >&2"));
        assert!(script.contains("\"$mkfs_bin\" -q -F -d \"$work_dir\" \"$rootfs\" >&2"));
        assert!(script.contains("\"$mkfs_bin\" -q -F \"$data\" >&2"));
        assert!(script.contains("printf '%s\\n' \"$rootfs\""));
        assert!(script.contains("printf '%s\\n' \"$data\""));
    }

    #[test]
    fn builds_boot_file_upload_script() {
        let script = upload_boot_file_script("web", "rootfs", "rootfs.ext4");

        assert!(script.contains("image_dir=\"$data_root/images\""));
        assert!(script.contains("destination=\"$image_dir/web-rootfs-rootfs.ext4\""));
        assert!(script.contains("cat > \"$tmp\""));
        assert!(script.contains("mv \"$tmp\" \"$destination\""));
        assert!(script.contains("printf '%s\\n' \"$destination\""));
    }

    #[test]
    fn builds_remote_tap_validation_script() {
        let script = require_tap_interface_script("tap-web");

        assert!(script.contains("tap='tap-web'"));
        assert!(script.contains("ip link show dev \"$tap\""));
        assert!(script.contains("grep -q '<[^>]*UP'"));
        assert!(script.contains("printf '%s\\n' \"$tap\""));
    }

    #[test]
    fn builds_remote_tap_ensure_script() {
        let script = ensure_tap_interface_script("tap-web");

        assert!(script.contains("ip link show dev \"$tap\""));
        assert!(script.contains("ip tuntap add dev \"$tap\" mode tap"));
        assert!(script.contains("ip addr replace 10.0.0.1/30 dev \"$tap\""));
        assert!(script.contains("ip link set \"$tap\" up"));
        assert!(script.contains("ip route replace 10.0.0.2/32 dev \"$tap\""));
        assert!(script.contains("printf '%s\\n' \"$tap\""));
    }

    #[test]
    fn builds_stale_tap_cleanup_script() {
        let script = cleanup_stale_tap_interfaces_script(&["t-active".to_string()]);

        assert!(script.contains("ip tuntap show"));
        assert!(script.contains("t-*|*-deploy"));
        assert!(script.contains("'t-active') continue"));
        assert!(script.contains("ip -brief link show dev \"$tap\""));
        assert!(script.contains("[ \"$state\" != \"DOWN\" ]"));
        assert!(script.contains("ip tuntap del dev \"$tap\" mode tap"));
        assert!(script.contains("printf '%s\\n' \"$tap\""));
    }

    #[test]
    fn builds_remote_runtime_dir_display() {
        assert_eq!(
            remote_runtime_dir_display("web"),
            "${XDG_RUNTIME_DIR:-${XDG_STATE_HOME:-$HOME/.local/state}/v/runtime}/v/web"
        );
    }

    #[test]
    fn builds_create_runtime_dirs_script() {
        let script = create_runtime_dirs_script("web");

        assert!(script.contains("XDG_DATA_HOME"));
        assert!(script.contains("XDG_STATE_HOME"));
        assert!(script.contains("XDG_RUNTIME_DIR"));
        assert!(script.contains("mkdir -p"));
        assert!(script.contains("$data_root/images"));
        assert!(script.contains("$data_root/volumes"));
        assert!(script.contains("printf '%s\\n' \"$runtime_dir\""));
    }

    #[test]
    fn builds_remote_firecracker_start_script() {
        let script = start_firecracker_script(
            "/usr/local/bin/firecracker",
            "/run/v/web/firecracker.sock",
            "/run/v/web/logs",
        );

        assert!(script.contains("nohup \"$firecracker_bin\" --api-sock \"$api_socket\""));
        assert!(script.contains("rm -f \"$api_socket\""));
        assert!(script.contains("[ -S \"$api_socket\" ]"));
        assert!(script.contains("printf '%s\\n' \"$pid\""));
    }

    #[test]
    fn builds_remote_firecracker_api_request_script() {
        let request =
            UnixHttpRequest::new("PUT", "/machine-config", r#"{"vcpu_count":1}"#.to_string())
                .expect("build request");
        let script = firecracker_api_request_script("/run/v/web/firecracker.sock", &request);

        assert!(script.contains("curl -sS -i --unix-socket \"$api_socket\""));
        assert!(script.contains("-X 'PUT'"));
        assert!(script.contains("'http://localhost/machine-config'"));
        assert!(script.contains(r#"'{"vcpu_count":1}'"#));
        assert!(script.contains("printf '%s\\n' \"$response\" >&2"));
    }

    #[test]
    fn builds_remote_firecracker_stop_script() {
        let script = stop_firecracker_script(4242, "/run/v/web");

        assert!(script.contains("pid=4242"));
        assert!(script.contains("kill -TERM \"$pid\""));
        assert!(script.contains("kill -KILL \"$pid\""));
        assert!(script.contains("rm -rf \"$runtime_dir\""));
    }

    #[test]
    fn builds_remote_runtime_rename_verification_script() {
        let script = runtime_dir_renamed_script("/run/v/web-deploy", "/run/v/web");

        assert!(script.contains("[ -d \"$new\" ] && [ ! -e \"$old\" ]"));
        assert!(script.contains("runtime rename did not complete"));
    }

    #[test]
    fn builds_remove_remote_runtime_dir_script() {
        let script = remove_remote_runtime_dir_script("/run/v/web-deploy");

        assert!(script.contains("rm -rf \"$runtime_dir\""));
        assert!(script.contains("runtime_dir='/run/v/web-deploy'"));
    }

    #[test]
    fn builds_remove_remote_runtime_for_app_script() {
        let script = remove_remote_runtime_for_app_script("web-deploy");

        assert!(script.contains("rm -rf \"$runtime_dir\""));
        assert!(script.contains("web-deploy"));
    }

    #[test]
    fn builds_health_check_script() {
        let script = health_check_script("10.0.0.2", 8080);

        assert!(script.contains("host='10.0.0.2'"));
        assert!(script.contains("port=8080"));
        assert!(script.contains("nc -z -w 5 \"$host\" \"$port\""));
    }

    #[test]
    fn builds_http_health_check_script() {
        let script = http_health_check_script("10.0.0.2", 8080, "/health");

        assert!(script.contains("host='10.0.0.2'"));
        assert!(script.contains("port=8080"));
        assert!(script.contains("path=/health"));
        assert!(script.contains("curl -sf --max-time 10 \"http://$host:$port$path\""));
    }

    #[test]
    fn http_health_check_strips_leading_slash_from_path() {
        let script = http_health_check_script("10.0.0.2", 3000, "/health");

        assert!(script.contains("path=/health"));
    }

    #[test]
    fn builds_caddy_config_update_script() {
        let script = update_caddy_config_script(
            "myapp",
            "myapp.example.com",
            "10.0.0.2",
            8080,
            "/etc/caddy/conf.d",
        );

        assert!(script.contains("config_file=\"$config_dir/$app.caddy\""));
        assert!(script.contains("sudo mkdir -p \"$config_dir\""));
        assert!(script.contains("sudo tee \"$config_file\""));
        assert!(script.contains("myapp.example.com {"));
        assert!(script.contains("reverse_proxy 10.0.0.2:8080"));
    }

    #[test]
    fn builds_caddy_config_remove_script() {
        let script = remove_caddy_config_script("myapp", "/etc/caddy/conf.d");

        assert!(script.contains("config_file=\"$config_dir/$app.caddy\""));
        assert!(script.contains("sudo rm -f \"$config_file\""));
        assert!(script.contains("no config to remove"));
    }

    #[test]
    fn builds_caddy_reload_script() {
        let script = reload_caddy_script();

        assert!(script.contains("sudo systemctl reload caddy"));
    }

    #[test]
    fn builds_check_caddy_script() {
        let script = check_caddy_script();

        assert!(script.contains("command -v caddy"));
        assert!(script.contains("systemctl is-active --quiet caddy"));
    }

    #[test]
    fn builds_worker_prerequisite_install_script() {
        let script =
            install_worker_prerequisites_script("/usr/local/bin/firecracker", "/etc/caddy");

        assert!(script.contains("sudo apt-get install -y -qq"));
        assert!(script.contains("sudo modprobe kvm"));
        assert!(script.contains("firecracker-microvm/firecracker/releases/latest"));
        assert!(script.contains("sudo install -m 0755"));
        assert!(script.contains("firecracker installed at $firecracker_bin"));
        assert!(script.contains("caddy_config_dir='/etc/caddy'"));
        assert!(script.contains("install_packages caddy"));
        assert!(script.contains("caddy ready with config dir $caddy_config_dir"));
    }

    #[test]
    fn builds_remote_dir_exists_script() {
        let script = check_remote_dir_exists_script("/run/v/web");

        assert!(script.contains("dir='/run/v/web'"));
        assert!(script.contains("[ -d \"$dir\" ]"));
        assert!(script.contains("printf 'exists\\n'"));
        assert!(script.contains("printf 'missing\\n'"));
    }
}

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

    #[allow(dead_code)]
    pub fn run_shell(&self, script: &str) -> Result<Output> {
        let output = self
            .shell_command(script)
            .output()
            .context("failed to run ssh command")?;

        if output.status.success() {
            return Ok(output);
        }

        bail!(
            "ssh command failed with status {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    pub fn check_capabilities(&self) -> Result<()> {
        self.run_shell(&worker_capability_script(&self.worker))
            .context("worker capability check failed")?;
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
                "ssh upload failed with status {}: {}",
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
        if let Err(error) = self.run_shell(&rename_runtime_dir_script(&old_path, &new_path)) {
            if self
                .run_shell(&runtime_dir_renamed_script(&old_path, &new_path))
                .is_err()
            {
                return Err(error).context(format!(
                    "failed to rename remote runtime dir {old_path} → {new_path}"
                ));
            }
        }
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

    pub fn stream_shell(&self, script: &str) -> Result<()> {
        let status = self
            .shell_command(script)
            .status()
            .context("failed to run ssh stream command")?;
        if !status.success() {
            bail!("ssh stream command failed with status {}", status);
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
        let config_dir = self
            .worker
            .caddy_config_dir
            .as_ref()
            .context("worker does not have caddy_config_dir configured")?;
        let config_dir = path_to_remote_string(config_dir)?;
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
        let config_dir = self
            .worker
            .caddy_config_dir
            .as_ref()
            .context("worker does not have caddy_config_dir configured")?;
        let config_dir = path_to_remote_string(config_dir)?;
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

    pub fn install_caddy(&self) -> Result<()> {
        self.run_shell(&install_caddy_script())
            .context("failed to install Caddy on worker")?;
        Ok(())
    }
}

pub fn remote_runtime_dir_display(app: &str) -> String {
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

fn worker_capability_script(worker: &WorkerConfig) -> String {
    let firecracker_bin = worker.firecracker_bin.as_str();

    format!(
        r#"set -eu
[ "$(uname -s)" = "Linux" ]
[ -e /dev/kvm ]
[ -r /dev/kvm ]
[ -w /dev/kvm ]
if [ -n "$(id -u)" ]; then :; fi
command -v sh >/dev/null
command -v ip >/dev/null
command -v curl >/dev/null
if [ -x {firecracker_bin} ]; then
  :
else
  command -v {firecracker_bin} >/dev/null
fi
"#,
        firecracker_bin = shell_quote(firecracker_bin)
    )
}

fn start_firecracker_script(firecracker_bin: &str, api_socket_path: &str, log_dir: &str) -> String {
    format!(
        r#"set -eu
firecracker_bin={firecracker_bin}
api_socket={api_socket_path}
log_dir={log_dir}
runtime_dir="$(dirname "$api_socket")"
mkdir -p "$runtime_dir" "$log_dir"
rm -f "$api_socket"
nohup "$firecracker_bin" --api-sock "$api_socket" > "$log_dir/firecracker.stdout.log" 2> "$log_dir/firecracker.stderr.log" < /dev/null &
pid=$!
i=0
while [ "$i" -lt 200 ]; do
  if [ -S "$api_socket" ]; then
    printf '%s\n' "$pid"
    exit 0
  fi
  if ! kill -0 "$pid" 2>/dev/null; then
    echo "Firecracker exited before creating API socket" >&2
    exit 1
  fi
  i=$((i + 1))
  sleep 0.025
done
kill "$pid" 2>/dev/null || true
echo "timed out waiting for Firecracker API socket: $api_socket" >&2
exit 1
"#,
        firecracker_bin = shell_quote(firecracker_bin),
        api_socket_path = shell_quote(api_socket_path),
        log_dir = shell_quote(log_dir)
    )
}

fn firecracker_api_request_script(api_socket_path: &str, request: &UnixHttpRequest) -> String {
    format!(
        r#"set -eu
api_socket={api_socket_path}
response="$(curl -sS -i --unix-socket "$api_socket" \
  -X {method} \
  -H 'Accept: application/json' \
  -H 'Content-Type: application/json' \
  --data {body} \
  {url})"
status="$(printf '%s\n' "$response" | sed -n '1s/^HTTP\/[0-9.]* \([0-9][0-9][0-9]\).*/\1/p')"
case "$status" in
  2??) exit 0 ;;
  *)
    printf '%s\n' "$response" >&2
    exit 1
    ;;
esac
"#,
        api_socket_path = shell_quote(api_socket_path),
        method = shell_quote(&request.method),
        body = shell_quote(&request.body),
        url = shell_quote(&format!("http://localhost{}", request.path))
    )
}

fn stop_firecracker_script(pid: u32, remote_runtime_dir: &str) -> String {
    format!(
        r#"set -eu
pid={pid}
runtime_dir={remote_runtime_dir}
if kill -0 "$pid" 2>/dev/null; then
  kill -TERM "$pid" 2>/dev/null || true
fi
i=0
while [ "$i" -lt 100 ]; do
  if ! kill -0 "$pid" 2>/dev/null; then
    break
  fi
  i=$((i + 1))
  sleep 0.025
done
if kill -0 "$pid" 2>/dev/null; then
  kill -KILL "$pid" 2>/dev/null || true
fi
rm -rf "$runtime_dir"
"#,
        pid = pid,
        remote_runtime_dir = shell_quote(remote_runtime_dir)
    )
}

fn rename_runtime_dir_script(old_path: &str, new_path: &str) -> String {
    format!(
        r#"set -eu
old={old_path}
new={new_path}
if [ ! -d "$old" ]; then
  echo "source directory does not exist: $old" >&2
  exit 1
fi
if [ -d "$new" ]; then
  rm -rf "$new"
fi
mkdir -p "$(dirname "$new")"
mv "$old" "$new"
printf '%s\n' "$new"
"#,
        old_path = shell_quote(old_path),
        new_path = shell_quote(new_path),
    )
}

fn runtime_dir_renamed_script(old_path: &str, new_path: &str) -> String {
    format!(
        r#"set -eu
old={old_path}
new={new_path}
if [ -d "$new" ] && [ ! -e "$old" ]; then
  printf '%s\n' "$new"
else
  echo "runtime rename did not complete: $old -> $new" >&2
  exit 1
fi
"#,
        old_path = shell_quote(old_path),
        new_path = shell_quote(new_path),
    )
}

fn require_readable_file_script(name: &str, path: &str) -> String {
    format!(
        r#"set -eu
name={name}
input={path}
case "$input" in
  '$XDG_DATA_HOME/'*)
    suffix="${{input#\$XDG_DATA_HOME/}}"
    base="${{XDG_DATA_HOME:-$HOME/.local/share}}"
    resolved="$base/$suffix"
    ;;
  '$XDG_STATE_HOME/'*)
    suffix="${{input#\$XDG_STATE_HOME/}}"
    base="${{XDG_STATE_HOME:-$HOME/.local/state}}"
    resolved="$base/$suffix"
    ;;
  '$XDG_RUNTIME_DIR/'*)
    suffix="${{input#\$XDG_RUNTIME_DIR/}}"
    if [ -z "${{XDG_RUNTIME_DIR:-}}" ]; then
      echo "$name uses XDG_RUNTIME_DIR but it is not set" >&2
      exit 1
    fi
    resolved="$XDG_RUNTIME_DIR/$suffix"
    ;;
  /*)
    resolved="$input"
    ;;
  *)
    echo "$name must be an absolute path or start with \$XDG_DATA_HOME/, \$XDG_STATE_HOME/, or \$XDG_RUNTIME_DIR/" >&2
    exit 1
    ;;
esac
[ -f "$resolved" ] || {{ echo "$name does not exist or is not a file: $resolved" >&2; exit 1; }}
[ -r "$resolved" ] || {{ echo "$name is not readable: $resolved" >&2; exit 1; }}
printf '%s\n' "$resolved"
"#,
        name = shell_quote(name),
        path = shell_quote(path)
    )
}

fn upload_boot_file_script(app: &str, kind: &str, filename: &str) -> String {
    format!(
        r#"set -eu
xdg_data_home="${{XDG_DATA_HOME:-$HOME/.local/share}}"
data_root="$xdg_data_home/v"
image_dir="$data_root/images"
mkdir -p "$image_dir"
destination="$image_dir/{app}-{kind}-{filename}"
tmp="$destination.tmp.$$"
cat > "$tmp"
mv "$tmp" "$destination"
printf '%s\n' "$destination"
"#,
        app = shell_escape_for_double_quotes(app),
        kind = shell_escape_for_double_quotes(kind),
        filename = shell_escape_for_double_quotes(filename)
    )
}

fn require_tap_interface_script(tap: &str) -> String {
    format!(
        r#"set -eu
tap={tap}
ip link show dev "$tap" >/dev/null
ip link show dev "$tap" | grep -q '<[^>]*UP'
printf '%s\n' "$tap"
"#,
        tap = shell_quote(tap)
    )
}

fn ensure_tap_interface_script(tap: &str) -> String {
    format!(
        r#"set -eu
tap={tap}
if ! ip link show dev "$tap" >/dev/null 2>&1; then
  ip tuntap add dev "$tap" mode tap
fi
ip link set "$tap" up
printf '%s\n' "$tap"
"#,
        tap = shell_quote(tap)
    )
}

fn delete_tap_interface_script(tap: &str) -> String {
    format!(
        r#"set -eu
tap={tap}
if ip link show dev "$tap" >/dev/null 2>&1; then
  ip link set "$tap" down 2>/dev/null || true
  ip tuntap del dev "$tap" mode tap
fi
"#,
        tap = shell_quote(tap)
    )
}

fn create_runtime_dirs_script(app: &str) -> String {
    format!(
        r#"set -eu
xdg_data_home="${{XDG_DATA_HOME:-$HOME/.local/share}}"
xdg_state_home="${{XDG_STATE_HOME:-$HOME/.local/state}}"
if [ -n "${{XDG_RUNTIME_DIR:-}}" ]; then
  runtime_root="$XDG_RUNTIME_DIR/v"
else
  runtime_root="$xdg_state_home/v/runtime"
fi
data_root="$xdg_data_home/v"
state_root="$xdg_state_home/v"
runtime_dir="$runtime_root/{app}"
log_dir="$runtime_dir/logs"
mkdir -p "$data_root/images" "$data_root/volumes" "$state_root" "$runtime_dir" "$log_dir"
printf '%s\n' "$runtime_dir"
"#,
        app = shell_escape_for_double_quotes(app)
    )
}

fn logs_script(log_dir: &str) -> String {
    format!(
        r#"set -eu
log_dir={log_dir}
if [ -d "$log_dir" ]; then
  for log in "$log_dir"/*.log; do
    if [ -f "$log" ]; then
      printf '=== %s ===\n' "$(basename "$log")"
      cat "$log"
    fi
  done
fi
"#,
        log_dir = shell_quote(log_dir)
    )
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

fn health_check_script(host: &str, port: u16) -> String {
    format!(
        r#"set -eu
host={host}
port={port}
if command -v nc >/dev/null 2>&1; then
  nc -z -w 5 "$host" "$port"
elif command -v bash >/dev/null 2>&1 && bash -c "timeout 5 bash -c '</dev/tcp/$host/$port' 2>/dev/null"; then
  :
elif command -v curl >/dev/null 2>&1; then
  curl -sf --max-time 5 "http://$host:$port" >/dev/null
else
  echo "no tool available for health check (nc, bash TCP, or curl)" >&2
  exit 1
fi
"#,
        host = shell_quote(host),
        port = port,
    )
}

fn http_health_check_script(host: &str, port: u16, path: &str) -> String {
    format!(
        r#"set -eu
host={host}
port={port}
path=/{path_clean}
if command -v curl >/dev/null 2>&1; then
  curl -sf --max-time 10 "http://$host:$port$path" >/dev/null
else
  echo "curl is required for HTTP health checks" >&2
  exit 1
fi
"#,
        host = shell_quote(host),
        port = port,
        path_clean = shell_escape_for_double_quotes(path.trim_start_matches('/')),
    )
}

fn update_caddy_config_script(
    app: &str,
    domain: &str,
    vm_host: &str,
    vm_port: u16,
    config_dir: &str,
) -> String {
    format!(
        r#"set -eu
app={app}
domain={domain}
target={target}
config_dir={config_dir}
config_file="$config_dir/$app.caddy"
sudo mkdir -p "$config_dir"
sudo tee "$config_file" >/dev/null <<'EOF'
{domain} {{
    reverse_proxy {vm_host}:{vm_port}
}}
EOF
printf '%s\n' "$config_file"
"#,
        app = shell_escape_for_double_quotes(app),
        domain = shell_escape_for_double_quotes(domain),
        target = shell_escape_for_double_quotes(&format!("{vm_host}:{vm_port}")),
        config_dir = shell_escape_for_double_quotes(config_dir),
        vm_host = shell_escape_for_double_quotes(vm_host),
        vm_port = vm_port,
    )
}

fn remove_caddy_config_script(app: &str, config_dir: &str) -> String {
    format!(
        r#"set -eu
app={app}
config_dir={config_dir}
config_file="$config_dir/$app.caddy"
if [ -f "$config_file" ]; then
  sudo rm -f "$config_file"
  printf '%s\n' "$config_file"
else
  printf '%s\n' "no config to remove for $app"
fi
"#,
        app = shell_escape_for_double_quotes(app),
        config_dir = shell_escape_for_double_quotes(config_dir),
    )
}

fn reload_caddy_script() -> String {
    r#"set -eu
sudo systemctl reload caddy
"#
    .to_string()
}

fn check_caddy_script() -> String {
    r#"set -eu
if command -v caddy >/dev/null 2>&1; then
  if systemctl is-active --quiet caddy 2>/dev/null; then
    printf '%s\n' "caddy: installed and running"
    exit 0
  fi
  caddy version >/dev/null 2>&1 && exit 0
fi
echo "caddy: not found or not running" >&2
exit 1
"#
    .to_string()
}

fn install_caddy_script() -> String {
    r#"set -eu
if command -v caddy >/dev/null 2>&1; then
  printf '%s\n' "caddy already installed"
  exit 0
fi
echo "installing Caddy via apt..."
sudo apt update -qq
sudo apt install -y -qq debian-keyring debian-archive-keyring apt-transport-https curl
curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key' \
  | sudo gpg --dearmor -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg
curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt' \
  | sudo tee /etc/apt/sources.list.d/caddy-stable.list
sudo apt update -qq
sudo apt install -y -qq caddy
sudo systemctl enable caddy
sudo systemctl start caddy
printf '%s\n' "caddy installed and started"
"#
    .to_string()
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
            caddy_config_dir: Some(PathBuf::from("/etc/caddy/conf.d")),
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
        assert!(script.contains("ip link set \"$tap\" up"));
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
    fn builds_install_caddy_script() {
        let script = install_caddy_script();

        assert!(script.contains("sudo apt install -y -qq caddy"));
        assert!(script.contains("sudo systemctl enable caddy"));
        assert!(script.contains("caddy-stable"));
    }
}

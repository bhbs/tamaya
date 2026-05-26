use crate::config::WorkerConfig;
use anyhow::{Context, Result, bail};
use std::ffi::OsString;
use std::path::Path;
use std::process::{Command, Output};

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
if [ -x {firecracker_bin} ]; then
  :
else
  command -v {firecracker_bin} >/dev/null
fi
"#,
        firecracker_bin = shell_quote(firecracker_bin)
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
}

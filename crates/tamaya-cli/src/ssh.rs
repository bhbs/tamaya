use crate::config::{HealthCheckConfig, WorkerConfig};
use crate::validation::PublishType;
use anyhow::{Context, Result, bail};
use std::ffi::OsString;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Cursor, Read, Write};
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::thread;

const PROGRESS_PREFIX: &str = "__TAMAYA_PROGRESS__";

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SshRunner {
    worker: WorkerConfig,
}

#[derive(Debug, Clone)]
pub struct CheckResult {
    pub success: bool,
    pub output: String,
}

#[derive(Debug, Clone)]
pub struct DeployParams<'a> {
    pub app: &'a str,
    pub domain: Option<&'a str>,
    pub path: Option<&'a str>,
    pub route_kind: &'a str,
    pub health: &'a HealthCheckConfig,
    pub memory_max: Option<&'a str>,
    pub cpu_quota: Option<&'a str>,
    pub writable_release: bool,
    pub verify_binary_deps: bool,
}

#[derive(Debug, Clone)]
pub struct PublishParams<'a> {
    pub app: &'a str,
    pub domain: &'a str,
    pub path: &'a str,
    pub route_kind: &'a str,
    pub publish_type: PublishType,
}

impl SshRunner {
    pub fn new(worker: WorkerConfig) -> Self {
        Self { worker }
    }

    pub fn command_args(&self, remote_command: &str) -> Vec<OsString> {
        [self.worker.alias.as_str(), remote_command]
            .map(OsString::from)
            .to_vec()
    }

    fn shell_command(&self, script: &str) -> Command {
        let mut command =
            Command::new(std::env::var_os("TAMAYA_SSH_BIN").unwrap_or_else(|| "ssh".into()));
        command.args(self.command_args(&format!("sh -lc {}", shell_quote(script))));
        command
    }

    pub fn run_shell(&self, script: &str) -> Result<Output> {
        crate::log::step(format!("connecting to {}", self.worker.alias));
        let output = self.output_with_progress(script)?;
        if !output.status.success() {
            bail!(
                "ssh command failed on {}: {}",
                self.worker.alias,
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        Ok(output)
    }

    pub fn stream_shell(&self, script: &str) -> Result<()> {
        crate::log::step(format!("connecting to {}", self.worker.alias));
        let mut child = self
            .shell_command(script)
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("failed to stream ssh command on {}", self.worker.alias))?;
        let stderr = child.stderr.take().context("failed to open ssh stderr")?;
        let stderr_thread = progress_thread(stderr, true);
        let status = child.wait().context("failed to wait for ssh stream")?;
        let stderr = stderr_thread
            .join()
            .map_err(|_| anyhow::anyhow!("failed to read ssh stderr"))??;
        if !status.success() {
            bail!(
                "ssh stream command failed on {}: {}",
                self.worker.alias,
                String::from_utf8_lossy(&stderr).trim()
            );
        }
        Ok(())
    }

    fn output_with_progress(&self, script: &str) -> Result<Output> {
        let mut child = self
            .shell_command(script)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("failed to run ssh command on {}", self.worker.alias))?;
        let stderr = child.stderr.take().context("failed to open ssh stderr")?;
        let stderr_thread = progress_thread(stderr, false);
        let mut output = child
            .wait_with_output()
            .context("failed to wait for ssh command")?;
        output.stderr = stderr_thread
            .join()
            .map_err(|_| anyhow::anyhow!("failed to read ssh stderr"))??;
        Ok(output)
    }

    pub fn setup(&self) -> Result<()> {
        validate_worker(&self.worker)?;
        self.run_shell(&setup_script(&self.worker)).map(|_| ())
    }

    pub fn check(&self) -> Result<CheckResult> {
        validate_worker(&self.worker)?;
        crate::log::step(format!("connecting to {}", self.worker.alias));
        let output = self.output_with_progress(&check_script(&self.worker))?;
        let stdout =
            String::from_utf8(output.stdout).context("worker check output is not UTF-8")?;
        Ok(CheckResult {
            success: output.status.success(),
            output: stdout,
        })
    }

    pub fn set_env(&self, app: &str, key: &str, value: &[u8]) -> Result<()> {
        validate_worker(&self.worker)?;
        validate_name("app", app)?;
        validate_name("environment variable key", key)?;
        self.pipe_bytes(
            &set_env_script(&self.worker, app, key),
            value,
            "environment variable",
        )
    }

    pub fn unset_env(&self, app: &str, key: &str) -> Result<()> {
        validate_worker(&self.worker)?;
        validate_name("app", app)?;
        validate_name("environment variable key", key)?;
        self.run_shell(&unset_env_script(&self.worker, app, key))
            .map(|_| ())
    }

    pub fn list_env(&self, app: &str) -> Result<String> {
        validate_worker(&self.worker)?;
        validate_name("app", app)?;
        let output = self.run_shell(&list_env_script(&self.worker, app))?;
        String::from_utf8(output.stdout).context("environment key list is not UTF-8")
    }

    pub fn deploy(&self, binary: &Path, params: &DeployParams<'_>) -> Result<String> {
        validate_worker(&self.worker)?;
        validate_params(params)?;
        let file = File::open(binary)
            .with_context(|| format!("failed to open binary {}", binary.display()))?;
        self.pipe_reader(&deploy_script(&self.worker, params), file, "binary deploy")
    }

    pub fn publish(&self, static_root: &Path, params: &PublishParams<'_>) -> Result<String> {
        validate_worker(&self.worker)?;
        validate_publish_params(params)?;
        let archive = site_archive(static_root)
            .with_context(|| format!("failed to archive static_root {}", static_root.display()))?;
        self.pipe_reader(
            &publish_script(&self.worker, params),
            Cursor::new(archive),
            "site publish",
        )
    }

    pub fn rollback(&self, app: &str) -> Result<String> {
        validate_worker(&self.worker)?;
        validate_name("app", app)?;
        let output = self.run_shell(&rollback_script(&self.worker, app))?;
        String::from_utf8(output.stdout).context("rollback output is not UTF-8")
    }

    pub fn status(&self, app: Option<&str>) -> Result<String> {
        validate_worker(&self.worker)?;
        if let Some(app) = app {
            validate_name("app", app)?;
        }
        let output = self.run_shell(&status_script(&self.worker, app))?;
        String::from_utf8(output.stdout).context("status output is not UTF-8")
    }

    pub fn stop(&self, app: &str) -> Result<()> {
        validate_worker(&self.worker)?;
        validate_name("app", app)?;
        self.run_shell(&stop_script(&self.worker, app)).map(|_| ())
    }

    pub fn delete(&self, app: &str, purge: bool) -> Result<()> {
        validate_worker(&self.worker)?;
        validate_name("app", app)?;
        self.run_shell(&delete_script(&self.worker, app, purge))
            .map(|_| ())
    }

    pub fn logs(&self, app: &str) -> Result<()> {
        validate_worker(&self.worker)?;
        validate_name("app", app)?;
        self.stream_shell(&logs_script(&self.worker, app))
    }

    pub fn maintenance(&self, app: &str, message: &str) -> Result<()> {
        validate_worker(&self.worker)?;
        validate_name("app", app)?;
        self.pipe_bytes(
            &maintenance_script(&self.worker, app),
            maintenance_html(message).as_bytes(),
            "maintenance page",
        )
    }

    pub fn live(&self, app: &str) -> Result<()> {
        validate_worker(&self.worker)?;
        validate_name("app", app)?;
        self.run_shell(&live_script(&self.worker, app)).map(|_| ())
    }

    pub fn maintenance_domain(&self, domain: &str, message: &str) -> Result<()> {
        validate_worker(&self.worker)?;
        validate_domain(domain)?;
        self.pipe_bytes(
            &maintenance_domain_script(&self.worker, domain),
            maintenance_html(message).as_bytes(),
            "maintenance page",
        )
    }

    pub fn live_domain(&self, domain: &str) -> Result<()> {
        validate_worker(&self.worker)?;
        validate_domain(domain)?;
        self.run_shell(&live_domain_script(&self.worker, domain))
            .map(|_| ())
    }

    fn pipe_bytes(&self, script: &str, bytes: &[u8], description: &str) -> Result<()> {
        self.pipe_reader(script, bytes, description).map(|_| ())
    }

    fn pipe_reader(
        &self,
        script: &str,
        mut reader: impl Read,
        description: &str,
    ) -> Result<String> {
        let mut child = self
            .shell_command(script)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("failed to start {description}"))?;
        let stderr = child.stderr.take().context("failed to open ssh stderr")?;
        let stderr_thread = progress_thread(stderr, false);
        let stream_result = {
            let mut stdin = child.stdin.take().context("failed to open ssh stdin")?;
            io::copy(&mut reader, &mut stdin)
        };
        let mut output = child
            .wait_with_output()
            .with_context(|| format!("failed to wait for {description}"))?;
        output.stderr = stderr_thread
            .join()
            .map_err(|_| anyhow::anyhow!("failed to read ssh stderr"))??;
        if let Err(error) = stream_result {
            if !output.status.success() {
                bail!(
                    "{description} failed: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                );
            }
            return Err(error).with_context(|| format!("failed to stream {description}"));
        }
        if !output.status.success() {
            bail!(
                "{description} failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        String::from_utf8(output.stdout)
            .with_context(|| format!("{description} output is not UTF-8"))
    }
}

fn site_archive(static_root: &Path) -> Result<Vec<u8>> {
    let mut archive = tar::Builder::new(Vec::new());
    append_site_entries(&mut archive, static_root, static_root)?;
    archive.finish()?;
    archive
        .into_inner()
        .context("failed to finish site archive")
}

fn append_site_entries(
    archive: &mut tar::Builder<Vec<u8>>,
    root: &Path,
    current: &Path,
) -> Result<()> {
    let mut entries = std::fs::read_dir(current)
        .with_context(|| format!("failed to read {}", current.display()))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.path());
    for entry in entries {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name == ".git" || name == ".env" {
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .with_context(|| format!("failed to relativize {}", path.display()))?;
        let metadata = entry
            .metadata()
            .with_context(|| format!("failed to stat {}", path.display()))?;
        if metadata.is_dir() {
            archive
                .append_dir(relative, &path)
                .with_context(|| format!("failed to add directory {}", path.display()))?;
            append_site_entries(archive, root, &path)?;
        } else if metadata.is_file() {
            archive
                .append_path_with_name(&path, relative)
                .with_context(|| format!("failed to add file {}", path.display()))?;
        }
    }
    Ok(())
}

fn progress_thread(
    stderr: impl Read + Send + 'static,
    stream_retained: bool,
) -> thread::JoinHandle<Result<Vec<u8>>> {
    thread::spawn(move || {
        let mut stderr = BufReader::new(stderr);
        let mut retained = Vec::new();
        loop {
            let mut line = Vec::new();
            if stderr.read_until(b'\n', &mut line)? == 0 {
                break;
            }
            if let Ok(text) = std::str::from_utf8(&line)
                && let Some(message) = text.trim_end().strip_prefix(PROGRESS_PREFIX)
            {
                crate::log::step(message);
                if message == "streaming logs" {
                    crate::log::stop_spinner();
                }
                continue;
            }
            if stream_retained {
                io::stderr().write_all(&line)?;
                io::stderr().flush()?;
            }
            retained.extend(line);
        }
        Ok(retained)
    })
}

fn validate_params(params: &DeployParams<'_>) -> Result<()> {
    validate_name("app", params.app)?;
    validate_route_kind(params.route_kind)?;
    if let Some(domain) = params.domain {
        validate_domain(domain)?;
    }
    if let Some(path) = params.path {
        validate_route_path(path)?;
        if params.domain.is_none() {
            bail!("path deploys require a domain");
        }
    }
    if !params.health.path.starts_with('/')
        || !params.health.path.bytes().all(|b| {
            b.is_ascii_alphanumeric() || matches!(b, b'/' | b'.' | b'_' | b'-' | b'?' | b'=' | b'&')
        })
    {
        bail!("health check path contains unsupported characters");
    }
    if params.health.retries == 0 {
        bail!("health check retries must be at least 1");
    }
    if params.health.timeout_secs == 0 {
        bail!("health check timeout must be at least 1 second");
    }
    validate_unit_value("memory.max", params.memory_max)?;
    validate_unit_value("cpu.quota", params.cpu_quota)?;
    Ok(())
}

fn validate_publish_params(params: &PublishParams<'_>) -> Result<()> {
    validate_name("app", params.app)?;
    validate_domain(params.domain)?;
    validate_route_path(params.path)?;
    validate_route_kind(params.route_kind)?;
    Ok(())
}

fn validate_route_kind(value: &str) -> Result<()> {
    if !matches!(value, "none" | "root" | "path") {
        bail!("route kind is invalid: {value:?}");
    }
    Ok(())
}

fn validate_worker(worker: &WorkerConfig) -> Result<()> {
    validate_remote_path("data_dir", &worker.data_dir)?;
    validate_remote_path("caddy_config_dir", &worker.caddy_config_dir)?;
    if worker.port_start == 0 || worker.port_start > worker.port_end {
        bail!("worker port range is invalid");
    }
    Ok(())
}

fn validate_remote_path(kind: &str, path: &Path) -> Result<()> {
    let value = path.to_string_lossy();
    if !path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'/' | b'.' | b'_' | b'-'))
    {
        bail!("{kind} must be an absolute path with shell-safe characters");
    }
    Ok(())
}

fn validate_unit_value(kind: &str, value: Option<&str>) -> Result<()> {
    if value.is_some_and(|v| {
        v.is_empty()
            || !v
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'%' | b'-'))
    }) {
        bail!("{kind} contains unsupported characters");
    }
    Ok(())
}

pub fn validate_name(kind: &str, value: &str) -> Result<()> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        bail!("{kind} must contain only ASCII letters, digits, '-' or '_': {value:?}");
    }
    Ok(())
}

fn validate_domain(value: &str) -> Result<()> {
    crate::validation::validate_domain(value)
}

fn validate_route_path(value: &str) -> Result<()> {
    if value == "/" {
        return Ok(());
    }
    if !value.starts_with('/')
        || value.ends_with('/')
        || value.contains("..")
        || value.contains("//")
        || value.contains('?')
        || value.contains('#')
        || value.bytes().any(|b| {
            !b.is_ascii_alphanumeric() && !matches!(b, b'/' | b'.' | b'_' | b'-' | b'~' | b'%')
        })
    {
        bail!("path must be an absolute URL path without trailing slash or unsafe characters");
    }
    Ok(())
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}
fn q(value: impl AsRef<str>) -> String {
    shell_quote(value.as_ref())
}
fn maintenance_html_template() -> &'static str {
    include_str!("scripts/maintenance.html")
}

fn maintenance_html(message: &str) -> String {
    let escaped = html_escape(message);
    maintenance_html_template().replace("{{message}}", &escaped)
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
fn data_dir(worker: &WorkerConfig) -> String {
    q(worker.data_dir.to_string_lossy())
}
fn caddy_dir(worker: &WorkerConfig) -> String {
    q(worker.caddy_config_dir.to_string_lossy())
}

fn with_progress(script: String) -> String {
    format!("progress() {{ printf '__TAMAYA_PROGRESS__%s\\n' \"$1\" >&2; }}\n{script}")
}

fn render(template: &str, replacements: &[(&str, &str)]) -> String {
    replacements
        .iter()
        .fold(template.to_owned(), |rendered, (key, value)| {
            rendered.replace(&format!("{{{{{key}}}}}"), value)
        })
}

fn setup_script(worker: &WorkerConfig) -> String {
    with_progress(render(
        include_str!("scripts/setup.sh"),
        &[("data", &data_dir(worker)), ("caddy", &caddy_dir(worker))],
    ))
}

fn check_script(worker: &WorkerConfig) -> String {
    with_progress(render(
        include_str!("scripts/check.sh"),
        &[("data", &data_dir(worker))],
    ))
}

fn set_env_script(worker: &WorkerConfig, app: &str, key: &str) -> String {
    with_progress(render(
        include_str!("scripts/env-set.sh"),
        &[
            ("app", app),
            ("key", key),
            ("data", &data_dir(worker)),
            ("metadata_helpers", metadata_helpers_script()),
        ],
    ))
}

fn unset_env_script(worker: &WorkerConfig, app: &str, key: &str) -> String {
    with_progress(render(
        include_str!("scripts/env-unset.sh"),
        &[
            ("app", app),
            ("key", key),
            ("data", &data_dir(worker)),
            ("metadata_helpers", metadata_helpers_script()),
        ],
    ))
}

fn list_env_script(worker: &WorkerConfig, app: &str) -> String {
    with_progress(render(
        include_str!("scripts/env-list.sh"),
        &[
            ("app", app),
            ("data", &data_dir(worker)),
            ("metadata_helpers", metadata_helpers_script()),
        ],
    ))
}

fn unit_body(
    worker: &WorkerConfig,
    app: &str,
    release_expr: &str,
    port_expr: &str,
    memory: Option<&str>,
    cpu: Option<&str>,
    writable_release: bool,
) -> String {
    let app_dir = format!("{}/apps/{app}", worker.data_dir.display());
    let mut body = render(
        include_str!("scripts/app.service"),
        &[
            ("app", app),
            ("app_dir", &app_dir),
            ("release", release_expr),
            ("port", port_expr),
        ],
    );
    if let Some(max) = memory {
        body.push_str(&format!("MemoryMax={max}\n"));
    }
    if let Some(quota) = cpu {
        body.push_str(&format!("CPUQuota={quota}\n"));
    }
    if writable_release {
        body.push_str(&format!(
            "ReadWritePaths={app_dir}/releases/{release_expr}\n"
        ));
    }
    body.push_str("\n[Install]\nWantedBy=multi-user.target\n");
    body
}

fn allocation_script(worker: &WorkerConfig) -> String {
    render(
        include_str!("scripts/allocate-port.sh"),
        &[
            ("start", &worker.port_start.to_string()),
            ("end", &worker.port_end.to_string()),
        ],
    )
}

fn verify_binary_deps_script() -> &'static str {
    include_str!("scripts/verify-binary-deps.sh")
}

fn caddy_shared_script() -> &'static str {
    concat!(
        include_str!("scripts/metadata.sh"),
        "\n",
        include_str!("scripts/caddy-shared.sh")
    )
}

fn metadata_helpers_script() -> &'static str {
    include_str!("scripts/metadata.sh")
}

fn app_units_script() -> &'static str {
    include_str!("scripts/app-units.sh")
}

fn deploy_script(worker: &WorkerConfig, p: &DeployParams<'_>) -> String {
    let domain = p.domain.unwrap_or("");
    let path = p.path.unwrap_or("");
    let unit = unit_body(
        worker,
        p.app,
        "$release",
        "$port",
        p.memory_max,
        p.cpu_quota,
        p.writable_release,
    );
    with_progress(render(
        include_str!("scripts/deploy.sh"),
        &[
            ("app", &q(p.app)),
            ("domain", &q(domain)),
            ("path", &q(path)),
            ("route_kind", &q(p.route_kind)),
            ("data", &data_dir(worker)),
            ("caddy", &caddy_dir(worker)),
            ("caddy_shared", caddy_shared_script()),
            (
                "health_check_failure",
                include_str!("scripts/health-check-failure.sh"),
            ),
            ("allocation", &allocation_script(worker)),
            (
                "writable_release_setup",
                if p.writable_release {
                    include_str!("scripts/writable-release.sh")
                } else {
                    ""
                },
            ),
            (
                "verify_binary_deps",
                if p.verify_binary_deps {
                    verify_binary_deps_script()
                } else {
                    ""
                },
            ),
            ("unit_body", &unit),
            ("retries", &p.health.retries.to_string()),
            ("timeout", &p.health.timeout_secs.to_string()),
            ("interval", &p.health.interval_secs.to_string()),
            ("health", &q(&p.health.path)),
        ],
    ))
}

fn publish_script(worker: &WorkerConfig, p: &PublishParams<'_>) -> String {
    with_progress(render(
        include_str!("scripts/publish.sh"),
        &[
            ("app", &q(p.app)),
            ("domain", &q(p.domain)),
            ("path", &q(p.path)),
            ("route_kind", &q(p.route_kind)),
            ("publish_type", &q(p.publish_type.to_string())),
            ("data", &data_dir(worker)),
            ("caddy", &caddy_dir(worker)),
            ("caddy_shared", caddy_shared_script()),
        ],
    ))
}

fn metadata_prelude(worker: &WorkerConfig, app: &str) -> String {
    with_progress(render(
        include_str!("scripts/metadata-prelude.sh"),
        &[
            ("app", &q(app)),
            ("data", &data_dir(worker)),
            ("caddy", &caddy_dir(worker)),
            ("app_units", app_units_script()),
            ("caddy_helpers", caddy_shared_script()),
        ],
    ))
}

fn rollback_script(worker: &WorkerConfig, app: &str) -> String {
    render(
        include_str!("scripts/rollback.sh"),
        &[
            ("prelude", &metadata_prelude(worker, app)),
            (
                "health_check_failure",
                include_str!("scripts/health-check-failure.sh"),
            ),
            ("allocation", &allocation_script(worker)),
        ],
    )
}

fn status_script(worker: &WorkerConfig, app: Option<&str>) -> String {
    let filter = app
        .map(|a| {
            format!(
                "apps={}",
                q(format!("{}/apps/{a}", worker.data_dir.display()))
            )
        })
        .unwrap_or_else(|| format!("apps={}/apps/*", q(worker.data_dir.to_string_lossy())));
    with_progress(render(
        include_str!("scripts/status.sh"),
        &[
            ("filter", &filter),
            ("metadata_helpers", metadata_helpers_script()),
        ],
    ))
}

fn remove_caddy() -> &'static str {
    include_str!("scripts/remove-caddy.sh")
}

fn stop_script(worker: &WorkerConfig, app: &str) -> String {
    render(
        include_str!("scripts/stop.sh"),
        &[
            ("prelude", &metadata_prelude(worker, app)),
            ("remove", remove_caddy()),
        ],
    )
}

fn delete_script(worker: &WorkerConfig, app: &str, purge: bool) -> String {
    with_progress(render(
        include_str!("scripts/delete.sh"),
        &[
            ("app", &q(app)),
            ("data", &data_dir(worker)),
            ("caddy", &caddy_dir(worker)),
            ("app_units", app_units_script()),
            ("caddy_shared", caddy_shared_script()),
            ("remove", remove_caddy()),
            (
                "delete_data",
                if purge {
                    include_str!("scripts/delete-purge.sh")
                } else {
                    include_str!("scripts/delete-retain-data.sh")
                },
            ),
        ],
    ))
}

fn logs_script(worker: &WorkerConfig, app: &str) -> String {
    render(
        include_str!("scripts/logs.sh"),
        &[("prelude", &metadata_prelude(worker, app))],
    )
}

fn maintenance_script(worker: &WorkerConfig, app: &str) -> String {
    render(
        include_str!("scripts/maintenance.sh"),
        &[("prelude", &metadata_prelude(worker, app))],
    )
}

fn live_script(worker: &WorkerConfig, app: &str) -> String {
    render(
        include_str!("scripts/live.sh"),
        &[("prelude", &metadata_prelude(worker, app))],
    )
}

fn maintenance_domain_script(worker: &WorkerConfig, domain: &str) -> String {
    with_progress(render(
        include_str!("scripts/maintenance-domain.sh"),
        &[
            ("domain", &q(domain)),
            ("data", &data_dir(worker)),
            ("caddy", &caddy_dir(worker)),
            ("caddy_shared", caddy_shared_script()),
        ],
    ))
}

fn live_domain_script(worker: &WorkerConfig, domain: &str) -> String {
    with_progress(render(
        include_str!("scripts/live-domain.sh"),
        &[
            ("domain", &q(domain)),
            ("data", &data_dir(worker)),
            ("caddy", &caddy_dir(worker)),
            ("caddy_shared", caddy_shared_script()),
        ],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn worker() -> WorkerConfig {
        WorkerConfig {
            alias: "host".into(),
            caddy_config_dir: PathBuf::from("/etc/caddy/conf.d"),
            data_dir: PathBuf::from("/var/lib/tamaya"),
            port_start: 20_000,
            port_end: 29_999,
        }
    }

    #[test]
    fn deploy_script_contains_systemd_hardening_and_port_allocation() {
        let health = HealthCheckConfig::default();
        let script = deploy_script(
            &worker(),
            &DeployParams {
                app: "web",
                domain: Some("web.test"),
                path: Some("/"),
                route_kind: "root",
                health: &health,
                memory_max: Some("512M"),
                cpu_quota: Some("50%"),
                writable_release: true,
                verify_binary_deps: false,
            },
        );
        assert!(script.contains("NoNewPrivileges=yes"));
        assert!(script.contains("WorkingDirectory=/var/lib/tamaya/apps/web/releases/$release"));
        assert!(script.contains("Environment=PORT=$port"));
        assert!(script.contains("Environment=HOSTNAME=127.0.0.1"));
        assert!(script.contains("MemoryMax=512M"));
        assert!(script.contains("CPUQuota=50%"));
        assert!(script.contains("ReadWritePaths=/var/lib/tamaya/apps/web/releases/$release"));
        assert!(script.contains("sudo chown -R \"tamaya-$app\":\"tamaya-$app\" \"$app_dir/data\""));
        let useradd_pos = script
            .find("sudo useradd --system --home \"$app_dir/data\"")
            .unwrap();
        let data_chown_pos = script
            .find("sudo chown -R \"tamaya-$app\":\"tamaya-$app\" \"$app_dir/data\"")
            .unwrap();
        assert!(useradd_pos < data_chown_pos);
        assert!(script.contains("! -path \"$app_dir/releases/$release/app\""));
        assert!(script.contains("sudo chown root:\"tamaya-$app\" \"$app_dir/releases/$release\""));
        assert!(script.contains("sudo chmod 1775 \"$app_dir/releases/$release\""));
        assert!(script.contains("sudo chown root:root \"$app_dir/releases/$release/app\""));
        assert!(script.contains("sudo chmod 0755 \"$app_dir/releases/$release/app\""));
        assert!(script.contains("health='/health'"));
        assert!(script.contains("\"http://127.0.0.1:$port$health\""));
        assert!(
            script.contains(
                "caddy_write_process_route_snippet \"$app\" \"$metadata_path\" \"$port\""
            )
        );
        assert!(script.contains("rebuild_domain \"$domain\""));
        assert!(script.contains("caddy_print_merged_domain_file \"$domain\""));
        assert!(script.contains("route_kind = \"$route_kind\""));
        assert!(script.contains("reverse_proxy 127.0.0.1:$write_port"));
        assert!(!script.contains("sudo tee \"$caddy_dir/$app.caddy.tmp\""));
        assert!(script.contains("sudo rm -f \"/etc/systemd/system/tamaya-$app-$release.service\""));
        assert!(script.contains("sudo systemctl reset-failed \"tamaya-$app-$release.service\""));
        assert!(!script.contains("checking binary dependencies"));
        assert!(script.contains("report_health_check_failure"));
        assert!(script.contains("journalctl -u \"$unit\" -n 40 --no-pager"));
    }

    #[test]
    fn deploy_script_includes_ldd_check_when_enabled() {
        let health = HealthCheckConfig::default();
        let script = deploy_script(
            &worker(),
            &DeployParams {
                app: "web",
                domain: None,
                path: None,
                route_kind: "none",
                health: &health,
                memory_max: None,
                cpu_quota: None,
                writable_release: false,
                verify_binary_deps: true,
            },
        );
        assert!(script.contains("checking binary dependencies"));
        assert!(script.contains("deploy aborted: install missing libraries"));
        assert!(script.contains(r#"binary="$staging/app""#));
        let deps_pos = script.find("checking binary dependencies").unwrap();
        let commit_pos = script
            .find(r#"sudo mv "$staging" "$app_dir/releases/$release""#)
            .unwrap();
        let systemd_pos = script.find("installing systemd service").unwrap();
        assert!(deps_pos < commit_pos);
        assert!(commit_pos < systemd_pos);
    }

    #[test]
    fn rollback_script_waits_for_health_and_cleans_up_on_failure() {
        let script = rollback_script(&worker(), "web");
        assert!(
            script.contains("health_retries=\"$(metadata_number \"$metadata\" health_retries)\"")
        );
        assert!(script.contains("for _ in $(seq 1 \"$health_retries\"); do"));
        assert!(script.contains("--max-time \"$health_timeout\""));
        assert!(script.contains("sleep \"$health_interval\""));
        assert!(script.contains("candidate_started=true"));
        assert!(script.contains("sudo systemctl disable --now \"$unit\""));
        assert!(script.contains("sudo mv \"$route_dir/$app.caddy.bak\""));
        assert!(script.contains("trap - EXIT"));
        assert!(script.contains("report_health_check_failure"));
        assert!(script.contains("journalctl -u \"$unit\" -n 40 --no-pager"));
    }

    #[test]
    fn rollback_script_has_published_branch_without_process_startup() {
        let script = rollback_script(&worker(), "docs");
        let branch_start = script
            .find("if test \"$app_type\" = \"published\"")
            .unwrap();
        let branch_end = script[branch_start..].find("exec 8>").unwrap() + branch_start;
        let branch = &script[branch_start..branch_end];
        assert!(branch.contains("previous_site_dir=\"$app_dir/releases/$previous/site\""));
        assert!(branch.contains("caddy_write_published_route_snippet \"$app\" \"$metadata_path\" \"$previous_site_dir\" \"$publish_type\""));
        assert!(branch.contains("app_type = \"published\""));
        assert!(branch.contains("site_dir = \"$previous_site_dir\""));
        assert!(branch.contains("printf 'rolled back %s to release %s\\n' \"$app\" \"$previous\""));
        assert!(!branch.contains("ports.lock"));
        assert!(!branch.contains("curl -fsS"));
        assert!(!branch.contains("systemctl enable --now \"$unit\""));
    }

    #[test]
    fn delete_script_can_purge_retained_data() {
        let script = delete_script(&worker(), "web", true);
        assert!(script.contains("test -d \"$app_dir\""));
        assert!(script.contains("if test -f \"$metadata\"; then validate_metadata_file"));
        assert!(script.contains("test \"$app_dir\" = \"$data_dir/apps/$app\""));
        assert!(script.contains("refusing to purge unexpected app directory"));
        assert!(script.contains("sudo rm -rf \"$app_dir\""));
        assert!(script.contains("sudo userdel \"tamaya-$app\""));
    }

    #[test]
    fn stop_and_delete_scripts_disable_release_units_safely() {
        let stop = stop_script(&worker(), "web");
        assert!(stop.contains("validate_app_name()"));
        assert!(stop.contains("is_release_unit_file()"));
        assert!(stop.contains("disable_release_units"));
        assert!(!stop.contains("for unit in $(systemctl"));

        let delete = delete_script(&worker(), "web", false);
        assert!(delete.contains("disable_release_units remove"));
        assert!(!delete.contains("for unit in $(systemctl"));
    }

    #[test]
    fn mutating_lifecycle_scripts_lock_and_replace_metadata_atomically() {
        let health = HealthCheckConfig::default();
        let scripts = [
            deploy_script(
                &worker(),
                &DeployParams {
                    app: "web",
                    domain: Some("web.test"),
                    path: Some("/"),
                    route_kind: "root",
                    health: &health,
                    memory_max: None,
                    cpu_quota: None,
                    writable_release: false,
                    verify_binary_deps: false,
                },
            ),
            publish_script(
                &worker(),
                &PublishParams {
                    app: "docs",
                    domain: "example.com",
                    path: "/docs",
                    route_kind: "path",
                    publish_type: PublishType::Static,
                },
            ),
            rollback_script(&worker(), "web"),
            stop_script(&worker(), "web"),
            maintenance_script(&worker(), "web"),
            live_script(&worker(), "web"),
            delete_script(&worker(), "web", false),
        ];

        for script in scripts {
            assert!(script.contains("acquire_app_operation_lock"));
            assert!(script.contains("app-locks/$app.lock"));
            assert!(!script.contains("sed -i"));
        }

        for script in [
            deploy_script(
                &worker(),
                &DeployParams {
                    app: "web",
                    domain: Some("web.test"),
                    path: Some("/"),
                    route_kind: "root",
                    health: &health,
                    memory_max: None,
                    cpu_quota: None,
                    writable_release: false,
                    verify_binary_deps: false,
                },
            ),
            publish_script(
                &worker(),
                &PublishParams {
                    app: "docs",
                    domain: "example.com",
                    path: "/docs",
                    route_kind: "path",
                    publish_type: PublishType::Static,
                },
            ),
            rollback_script(&worker(), "web"),
            stop_script(&worker(), "web"),
            maintenance_script(&worker(), "web"),
            live_script(&worker(), "web"),
        ] {
            assert!(script.contains("atomic_write_metadata"));
            assert!(script.contains(".metadata.toml.tmp.XXXXXX"));
            assert!(script.contains("chown root:root \"$metadata_tmp\""));
            assert!(script.contains("chmod 0600 \"$metadata_tmp\""));
            assert!(script.contains("mv \"$metadata_tmp\" \"$metadata_target\""));
        }
    }

    #[test]
    fn metadata_consumers_validate_before_privileged_use() {
        let helper = caddy_shared_script();
        assert!(helper.contains("validate_metadata_file()"));
        assert!(helper.contains("corrupted metadata in $1"));
        assert!(helper.contains("validate_metadata_file \"$rebuild_metadata\""));
        assert!(helper.contains("validate_metadata_file \"$check_other_metadata\""));

        let status = status_script(&worker(), None);
        assert!(status.contains("validate_metadata_file \"$metadata\" \"$expected_app\""));
        let logs = logs_script(&worker(), "web");
        assert!(logs.contains("validate_metadata_file \"$metadata\" \"$app\""));
        assert!(logs.contains("exec 6>&-"));
    }

    #[test]
    fn metadata_validator_rejects_corrupted_privileged_values() {
        let root = std::env::temp_dir().join(format!(
            "tamaya-metadata-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let app_dir = root.join("apps/web");
        std::fs::create_dir_all(app_dir.join("releases/20260606120000")).unwrap();
        let metadata = app_dir.join("metadata.toml");
        let valid = r#"app = "web"
current = "20260606120000"
previous = ""
app_type = "process"
unit = "tamaya-web-20260606120000.service"
port = 20000
domain = "example.com"
path = "/"
route_kind = "root"
status = "running"
health_path = "/health"
health_retries = 7
health_timeout = 3
health_interval = 2
publish_type = ""
site_dir = ""
"#
        .to_owned();
        std::fs::write(&metadata, &valid).unwrap();
        let helper = concat!(env!("CARGO_MANIFEST_DIR"), "/src/scripts/metadata.sh");
        let validate = |path: &Path| {
            Command::new("bash")
                .args([
                    "-c",
                    ". \"$1\"; validate_metadata_file \"$2\" web",
                    "bash",
                    helper,
                ])
                .arg(path)
                .output()
                .unwrap()
        };

        assert!(validate(&metadata).status.success());

        std::fs::write(
            &metadata,
            valid.replace(
                "tamaya-web-20260606120000.service",
                "../../attacker.service",
            ),
        )
        .unwrap();
        let rejected = validate(&metadata);
        assert!(!rejected.status.success());
        assert!(String::from_utf8_lossy(&rejected.stderr).contains("corrupted metadata"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn deploy_script_for_path_app_writes_route_snippet_and_rebuilds_domain() {
        let health = HealthCheckConfig::default();
        let script = deploy_script(
            &worker(),
            &DeployParams {
                app: "api",
                domain: Some("example.com"),
                path: Some("/api"),
                route_kind: "path",
                health: &health,
                memory_max: None,
                cpu_quota: None,
                writable_release: false,
                verify_binary_deps: false,
            },
        );
        assert!(script.contains("route_dir=\"$data_dir/caddy-routes\""));
        assert!(
            script.contains("ensure_route_compatible \"$app\" \"process\" \"$domain\" \"$path\"")
        );
        let capture_previous = script.find("old_unit=\"$md_unit\"").unwrap();
        let check_routes = script
            .find("ensure_route_compatible \"$app\" \"process\" \"$domain\" \"$path\"")
            .unwrap();
        assert!(capture_previous < check_routes);
        assert!(
            script.contains(
                "caddy_write_process_route_snippet \"$app\" \"$metadata_path\" \"$port\""
            )
        );
        assert!(script.contains("sudo tee \"$route_dir/$write_app.caddy.tmp\""));
        assert!(script.contains("$write_matcher path $write_match"));
        assert!(script.contains("handle $write_matcher {"));
        assert!(script.contains("reverse_proxy 127.0.0.1:$write_port"));
        assert!(script.contains("rebuild_domain \"$domain\""));
        assert!(script.contains("path = \"$metadata_path\""));
        assert!(!script.contains("handle_path"));
    }

    #[test]
    fn publish_script_writes_site_release_metadata_and_static_route() {
        let script = publish_script(
            &worker(),
            &PublishParams {
                app: "docs",
                domain: "example.com",
                path: "/docs",
                route_kind: "path",
                publish_type: PublishType::Static,
            },
        );
        assert!(script.contains("sudo tar -xf - -C \"$staging/site\""));
        assert!(script.contains("sudo find \"$staging/site\" -type d -exec chmod 0755 {} +"));
        assert!(script.contains("sudo find \"$staging/site\" -type f -exec chmod 0644 {} +"));
        assert!(script.contains("sudo chown -R root:root \"$staging/site\""));
        assert!(
            script.contains("ensure_route_compatible \"$app\" \"published\" \"$domain\" \"$path\"")
        );
        let capture_previous = script.find("old_release=\"$md_current\"").unwrap();
        let check_routes = script
            .find("ensure_route_compatible \"$app\" \"published\" \"$domain\" \"$path\"")
            .unwrap();
        assert!(capture_previous < check_routes);
        assert!(script.contains("app_type = \"published\""));
        assert!(script.contains("publish_type = \"$publish_type\""));
        assert!(script.contains("site_dir = \"$site_dir\""));
        assert!(script.contains(
            "caddy_write_published_route_snippet \"$app\" \"$metadata_path\" \"$site_dir\" \"$publish_type\""
        ));
        assert!(script.contains("try_files {path} {path}.html {path}/ /404.html"));
        assert!(script.contains(
            "ls -1dt \"$app_dir\"/releases/* 2>/dev/null | tail -n +6 | xargs -r sudo rm -rf"
        ));
        assert!(!script.contains("systemctl enable --now \"$unit\""));
        assert!(!script.contains("useradd --system"));
    }

    #[test]
    fn publish_script_writes_spa_fallback() {
        let script = publish_script(
            &worker(),
            &PublishParams {
                app: "docs",
                domain: "example.com",
                path: "/",
                route_kind: "root",
                publish_type: PublishType::Spa,
            },
        );
        assert!(script.contains("caddy_write_published_route_snippet \"$app\" \"$metadata_path\" \"$site_dir\" \"$publish_type\""));
        assert!(script.contains("try_files {path} /index.html"));
        assert!(script.contains("handle {"));
        assert!(script.contains("rebuild_domain \"$domain\""));
        assert!(script.contains("caddy_print_merged_domain_file \"$domain\""));
        assert!(!script.contains("$domain {\n    root * $site_dir"));
    }

    #[test]
    fn site_archive_excludes_sensitive_files_but_keeps_well_known() {
        let root = std::env::temp_dir().join(format!(
            "tamaya-archive-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join(".well-known")).unwrap();
        std::fs::create_dir_all(root.join(".git")).unwrap();
        std::fs::write(root.join("index.html"), "ok").unwrap();
        std::fs::write(root.join(".well-known/security.txt"), "contact").unwrap();
        std::fs::write(root.join(".env"), "SECRET=1").unwrap();
        std::fs::write(root.join(".git/config"), "secret").unwrap();

        let archive = site_archive(&root).unwrap();
        let mut names = tar::Archive::new(&archive[..])
            .entries()
            .unwrap()
            .map(|entry| {
                entry
                    .unwrap()
                    .path()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect::<Vec<_>>();
        names.sort();
        assert!(names.contains(&"index.html".to_owned()));
        assert!(names.contains(&".well-known".to_owned()));
        assert!(names.contains(&".well-known/security.txt".to_owned()));
        assert!(
            !names
                .iter()
                .any(|name| name == ".env" || name.starts_with(".git"))
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn lifecycle_scripts_for_path_apps_remove_snippets_and_rebuild_domain() {
        let rollback = rollback_script(&worker(), "api");
        assert!(
            rollback.contains(
                "caddy_write_process_route_snippet \"$app\" \"$metadata_path\" \"$port\""
            )
        );
        assert!(rollback.contains("$write_matcher path $write_match"));
        assert!(rollback.contains("handle $write_matcher {"));
        assert!(rollback.contains("rebuild_domain \"$domain\""));
        assert!(!rollback.contains("handle_path"));

        for script in [
            stop_script(&worker(), "api"),
            delete_script(&worker(), "api", false),
        ] {
            assert!(script.contains("route_dir=\"$data_dir/caddy-routes\""));
            assert!(script.contains("rebuild_domain \"$domain\""));
            assert!(script.contains("sudo rm -f \"$route_dir/$app.caddy\""));
            assert!(!script.contains("handle_path"));
        }
    }

    #[test]
    fn shared_caddy_rebuild_sorts_cleans_up_and_restores_on_failure() {
        let helper = caddy_shared_script();
        assert!(helper.contains("sort -rn -k1,1 \"$rebuild_path_list\""));
        assert!(helper.contains("sed 's/^/    /' \"$route_dir/$rebuild_root_app.caddy\""));
        assert!(helper.contains("caddy_remove_domain_file \"$rebuild_target\""));
        assert!(helper.contains("sudo caddy validate --config /etc/caddy/Caddyfile"));
        assert!(helper.contains("caddy_restore_domain_file \"$validate_out\""));
        assert!(helper.contains("sudo mv \"$restore_bak\" \"$restore_out\""));
        assert!(helper.contains("sudo rm -f \"$restore_out\" \"$restore_bak\""));
        assert!(helper.contains("caddy_remove_stale_standalone_files_for_domain()"));
        assert!(helper.contains("caddy_site_block_header()"));
        assert!(helper.contains(r#"test "$stale_header" = "$remove_domain""#));
        assert!(helper.contains("caddy_print_domain_routes()"));
        assert!(helper.contains("Routes (%s)"));
        assert!(helper.contains("caddy_print_merged_domain_file()"));
        assert!(helper.contains("sudo chmod 0600 \"$lock_dir/caddy.lock\""));
        assert!(helper.contains("sudo chown root:root \"$replace_tmp\""));
        assert!(helper.contains("sudo chmod 0644 \"$replace_tmp\""));
        assert!(!helper.contains("chmod 0666"));
        assert!(helper.contains("ensure_route_compatible()"));
        assert!(helper.contains("already has root route"));
        assert!(helper.contains("already has path route"));
        assert!(helper.contains("delete it before deploying as $check_app_type"));
    }

    #[test]
    fn shared_caddy_route_snippets_are_site_block_directives() {
        let helper = caddy_shared_script();
        let snippet_pos = helper.find("caddy_write_process_route_snippet()").unwrap();
        let rebuild_pos = helper.find("rebuild_domain()").unwrap();
        let snippet = &helper[snippet_pos..rebuild_pos];
        assert!(snippet.contains("$write_matcher path $write_match"));
        assert!(snippet.contains("handle $write_matcher {"));
        assert!(snippet.contains("handle {"));
        assert!(snippet.contains("reverse_proxy 127.0.0.1:$write_port"));
        assert!(!snippet.contains("$domain {"));
        assert!(!snippet.contains("$rebuild_domain_value {"));

        let delete = delete_script(&worker(), "api", false);
        assert!(
            delete.contains("sudo rm -f \"$domain_dir/$(domain_key \"$domain\").maintenance\"")
        );
        assert!(
            delete
                .contains("sudo rm -rf \"$data_dir/static/maintenance/$(domain_key \"$domain\")\"")
        );
    }

    #[test]
    fn domain_maintenance_scripts_use_domain_state_and_rebuild() {
        let maintenance = maintenance_domain_script(&worker(), "http://example.com");
        assert!(maintenance.contains("domain='http://example.com'"));
        assert!(maintenance.contains("Tamaya has no known apps for $domain"));
        assert!(maintenance.contains("sudo tee \"$domain_dir/$domain_key_value.maintenance\""));
        assert!(!maintenance.contains("Back shortly"));
        assert!(maintenance.contains("rebuild_domain \"$domain\""));
        assert!(maintenance.contains("static/maintenance"));
        assert!(maintenance.contains("__DOMAIN__"));

        let live = live_domain_script(&worker(), "http://example.com");
        assert!(live.contains("sudo test -f \"$domain_dir/$domain_key_value.maintenance\""));
        assert!(live.contains("is not in maintenance"));
        assert!(live.contains("sudo rm -f \"$domain_dir/$domain_key_value.maintenance\""));
        assert!(live.contains("sudo rm -rf \"$data_dir/static/maintenance/$domain_key_value\""));
        assert!(live.contains("rebuild_domain \"$domain\""));
    }

    #[test]
    fn logs_and_env_scripts_reject_published_apps_explicitly() {
        let logs = logs_script(&worker(), "docs");
        assert!(logs.contains("does not have systemd logs"));
        assert!(logs.contains("app_type=\"$(value app_type)\""));

        for script in [
            set_env_script(&worker(), "docs", "TOKEN"),
            unset_env_script(&worker(), "docs", "TOKEN"),
            list_env_script(&worker(), "docs"),
        ] {
            assert!(script.contains("metadata=\"$data_dir/apps/$app/metadata.toml\""));
            assert!(script.contains("does not support environment variables"));
        }
    }

    #[test]
    fn check_script_includes_tar_for_publish() {
        assert!(check_script(&worker()).contains("for cmd in ss flock curl tar; do"));
    }

    #[test]
    fn scripts_do_not_leave_template_placeholders() {
        let health = HealthCheckConfig::default();
        let scripts = [
            setup_script(&worker()),
            check_script(&worker()),
            set_env_script(&worker(), "web", "TOKEN"),
            unset_env_script(&worker(), "web", "TOKEN"),
            list_env_script(&worker(), "web"),
            deploy_script(
                &worker(),
                &DeployParams {
                    app: "web",
                    domain: Some("web.test"),
                    path: Some("/"),
                    route_kind: "root",
                    health: &health,
                    memory_max: None,
                    cpu_quota: None,
                    writable_release: true,
                    verify_binary_deps: false,
                },
            ),
            rollback_script(&worker(), "web"),
            publish_script(
                &worker(),
                &PublishParams {
                    app: "docs",
                    domain: "example.com",
                    path: "/docs",
                    route_kind: "path",
                    publish_type: PublishType::Static,
                },
            ),
            status_script(&worker(), None),
            stop_script(&worker(), "web"),
            delete_script(&worker(), "web", false),
            logs_script(&worker(), "web"),
            maintenance_script(&worker(), "web"),
            live_script(&worker(), "web"),
            maintenance_domain_script(&worker(), "example.com"),
            live_domain_script(&worker(), "example.com"),
        ];
        for script in scripts {
            assert!(!script.contains("{{"), "unresolved template in:\n{script}");
        }
    }

    #[test]
    fn validation_rejects_shell_input() {
        assert!(validate_domain("web.example.com").is_ok());
        assert!(validate_domain("http://web.example.com").is_ok());
        assert!(validate_domain("").is_err());
        assert!(validate_domain("http://").is_err());
        assert!(validate_domain("https://web.example.com").is_err());
        assert!(validate_domain(".").is_err());
        assert!(validate_domain("..").is_err());
        assert!(validate_domain(".example.com").is_err());
        assert!(validate_domain("example.com.").is_err());
        assert!(validate_domain("-example.com").is_err());
        assert!(validate_domain("example-.com").is_err());
        assert!(validate_name("app", "bad;rm").is_err());
        assert!(validate_name("app", "").is_err());
        assert!(validate_domain("bad domain").is_err());
        let mut bad_worker = worker();
        bad_worker.data_dir = PathBuf::from("/tmp/$(touch bad)");
        assert!(validate_worker(&bad_worker).is_err());
        bad_worker = worker();
        bad_worker.caddy_config_dir = PathBuf::from("relative");
        assert!(validate_worker(&bad_worker).is_err());
        bad_worker = worker();
        bad_worker.port_start = 0;
        assert!(validate_worker(&bad_worker).is_err());
        bad_worker = worker();
        bad_worker.port_start = 30_000;
        bad_worker.port_end = 20_000;
        assert!(validate_worker(&bad_worker).is_err());
        assert!(validate_unit_value("memory.max", None).is_ok());
        assert!(validate_unit_value("memory.max", Some("512M")).is_ok());
        assert!(validate_unit_value("memory.max", Some("")).is_err());
        assert!(validate_unit_value("memory.max", Some("bad value")).is_err());
        let health = HealthCheckConfig {
            path: "/bad$path".into(),
            ..HealthCheckConfig::default()
        };
        assert!(
            validate_params(&DeployParams {
                app: "web",
                domain: None,
                path: None,
                route_kind: "none",
                health: &health,
                memory_max: None,
                cpu_quota: None,
                writable_release: false,
                verify_binary_deps: false,
            })
            .is_err()
        );
        let health = HealthCheckConfig {
            path: "health".into(),
            ..HealthCheckConfig::default()
        };
        assert!(
            validate_params(&DeployParams {
                app: "web",
                domain: None,
                path: None,
                route_kind: "none",
                health: &health,
                memory_max: None,
                cpu_quota: None,
                writable_release: false,
                verify_binary_deps: false,
            })
            .is_err()
        );
        let health = HealthCheckConfig::default();
        assert!(
            validate_params(&DeployParams {
                app: "web",
                domain: Some("web.example.com"),
                path: Some("/"),
                route_kind: "root",
                health: &health,
                memory_max: Some("512M"),
                cpu_quota: Some("50%"),
                writable_release: false,
                verify_binary_deps: false,
            })
            .is_ok()
        );
    }

    #[test]
    fn command_args_use_worker_alias() {
        let args = SshRunner::new(worker()).command_args("true");
        assert_eq!(args, ["host", "true"].map(OsString::from));
    }

    #[test]
    fn quoting_handles_shell_special_characters() {
        assert_eq!(shell_quote("it's"), "'it'\"'\"'s'");
    }

    #[test]
    fn maintenance_html_is_data_not_generated_shell_source() {
        let payload = "planned\nEOF\nsudo touch /tmp/injected";
        let html = maintenance_html(payload);
        assert!(html.contains(payload));

        let script = maintenance_script(&worker(), "web");
        assert!(!script.contains(payload));
        assert!(
            script.contains("sudo tee \"$domain_dir/$domain_key_value.maintenance\" >/dev/null")
        );
        assert!(!include_str!("scripts/maintenance.sh").contains("<<EOF\n$message"));
        assert!(!include_str!("scripts/maintenance-domain.sh").contains("<<"));
    }

    #[test]
    fn deploy_reports_missing_file() {
        let runner = SshRunner::new(worker());
        let health = HealthCheckConfig::default();
        assert!(
            runner
                .deploy(
                    Path::new("/definitely/missing/tamaya-binary"),
                    &DeployParams {
                        app: "web",
                        domain: None,
                        path: None,
                        route_kind: "none",
                        health: &health,
                        memory_max: None,
                        cpu_quota: None,
                        writable_release: false,
                        verify_binary_deps: false,
                    }
                )
                .is_err()
        );
        assert!(
            runner
                .publish(
                    Path::new("/definitely/missing/tamaya-static-root"),
                    &PublishParams {
                        app: "docs",
                        domain: "example.com",
                        path: "/docs",
                        route_kind: "path",
                        publish_type: PublishType::Static,
                    }
                )
                .is_err()
        );
    }

    #[test]
    fn methods_reject_invalid_names_and_workers_before_ssh() {
        let runner = SshRunner::new(worker());
        assert!(runner.set_env("bad app", "TOKEN", b"").is_err());
        assert!(runner.set_env("web", "bad key", b"").is_err());
        assert!(runner.unset_env("bad app", "TOKEN").is_err());
        assert!(runner.list_env("bad app").is_err());
        assert!(runner.rollback("bad app").is_err());
        assert!(runner.status(Some("bad app")).is_err());
        assert!(runner.stop("bad app").is_err());
        assert!(runner.delete("bad app", false).is_err());
        assert!(runner.logs("bad app").is_err());
        assert!(runner.maintenance("bad app", "message").is_err());
        assert!(runner.live("bad app").is_err());
        assert!(runner.maintenance_domain("bad domain", "message").is_err());
        assert!(runner.live_domain("bad domain").is_err());
        assert!(
            runner
                .publish(
                    Path::new("/tmp"),
                    &PublishParams {
                        app: "bad app",
                        domain: "example.com",
                        path: "/docs",
                        route_kind: "path",
                        publish_type: PublishType::Static,
                    }
                )
                .is_err()
        );

        let mut invalid = worker();
        invalid.port_start = 0;
        let runner = SshRunner::new(invalid);
        assert!(runner.setup().is_err());
        assert!(runner.check().is_err());
        assert!(runner.rollback("web").is_err());
        assert!(runner.status(None).is_err());
        assert!(runner.stop("web").is_err());
        assert!(runner.delete("web", false).is_err());
        assert!(runner.logs("web").is_err());
        assert!(runner.maintenance("web", "message").is_err());
        assert!(runner.live("web").is_err());
    }

    #[test]
    fn shell_command_defaults_to_ssh() {
        unsafe { std::env::remove_var("TAMAYA_SSH_BIN") };
        assert_eq!(
            SshRunner::new(worker()).shell_command("true").get_program(),
            "ssh"
        );
    }
}

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "v")]
#[command(about = "Lightweight Firecracker PaaS control CLI")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Initialize local controller directories and config.
    Init,
    /// Build a Linux worker Firecracker boot plan for an app.
    Run {
        app: String,
        #[arg(long)]
        worker: Option<String>,
        #[arg(long)]
        kernel: PathBuf,
        #[arg(long)]
        rootfs: PathBuf,
        #[arg(long, default_value = "firecracker")]
        firecracker_bin: PathBuf,
        #[arg(long, default_value = "tap0")]
        tap: String,
        #[arg(long, default_value = "console=ttyS0 reboot=k panic=1 pci=off")]
        boot_args: String,
        #[arg(long, default_value_t = 1)]
        vcpu: u8,
        #[arg(long, default_value_t = 256)]
        memory_mib: u32,
        #[arg(long)]
        dry_run: bool,
    },
    /// Check Linux worker readiness and optionally validate remote files.
    Check {
        app: String,
        #[arg(long)]
        worker: Option<String>,
        #[arg(long)]
        kernel: Option<PathBuf>,
        #[arg(long)]
        rootfs: Option<PathBuf>,
        #[arg(long, default_value = "tap0")]
        tap: String,
        #[arg(long)]
        skip_runtime: bool,
        #[arg(long)]
        skip_capabilities: bool,
        #[arg(long)]
        skip_kernel: bool,
        #[arg(long)]
        skip_rootfs: bool,
        #[arg(long)]
        skip_tap: bool,
        #[arg(long)]
        skip_caddy: bool,
    },
    /// Deploy an immutable app image.
    Deploy {
        app: String,
        #[arg(long)]
        worker: Option<String>,
        #[arg(long)]
        kernel: PathBuf,
        #[arg(long)]
        rootfs: PathBuf,
        #[arg(long, default_value = "firecracker")]
        firecracker_bin: PathBuf,
        #[arg(long, default_value = "tap0")]
        tap: String,
        #[arg(long, default_value = "console=ttyS0 reboot=k panic=1 pci=off")]
        boot_args: String,
        #[arg(long, default_value_t = 1)]
        vcpu: u8,
        #[arg(long, default_value_t = 256)]
        memory_mib: u32,
        #[arg(long, default_value = "10.0.0.2")]
        health_check_host: String,
        #[arg(long)]
        health_check_path: Option<String>,
        #[arg(long, default_value_t = 10)]
        health_check_retries: u32,
        #[arg(long, default_value_t = 2)]
        health_check_interval_secs: u32,
        #[arg(long, default_value_t = 5)]
        drain_seconds: u32,
        #[arg(long)]
        skip_health_check: bool,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        domain: Option<String>,
    },
    /// Roll back an app to the previous image.
    Rollback { app: String },
    /// List managed microVMs.
    Ps,
    /// Stop an app microVM.
    Stop { app: String },
    /// Show app logs.
    Logs { app: String },
    /// Install and configure worker prerequisites (Firecracker, Caddy, etc).
    Setup {
        #[arg(long)]
        worker: Option<String>,
        #[arg(long)]
        caddy: bool,
    },
    /// Force-clean up stale lock files for an app.
    Unlock { app: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use std::path::Path;

    #[test]
    fn parses_init_command() {
        let cli = Cli::try_parse_from(["v", "init"]).expect("parse init command");

        assert!(matches!(cli.command, Command::Init));
    }

    #[test]
    fn parses_deploy_app_argument() {
        let cli = Cli::try_parse_from([
            "v",
            "deploy",
            "myapp",
            "--kernel",
            "/kernels/vmlinux",
            "--rootfs",
            "/images/myapp-v2.ext4",
            "--worker",
            "vps-prod",
            "--vcpu",
            "2",
            "--memory-mib",
            "512",
            "--tap",
            "tap-web",
        ])
        .expect("parse deploy command");

        assert!(matches!(cli.command, Command::Deploy {
                app,
                worker,
                kernel,
                rootfs,
                tap,
                vcpu,
                memory_mib,
                dry_run,
                ..
            }
                if app == "myapp"
                    && worker.as_deref() == Some("vps-prod")
                    && kernel == Path::new("/kernels/vmlinux")
                    && rootfs == Path::new("/images/myapp-v2.ext4")
                    && tap == "tap-web"
                    && vcpu == 2
                    && memory_mib == 512
                    && !dry_run));
    }

    #[test]
    fn parses_deploy_with_dry_run() {
        let cli = Cli::try_parse_from([
            "v",
            "deploy",
            "myapp",
            "--kernel",
            "/kernels/vmlinux",
            "--rootfs",
            "/images/app.ext4",
            "--dry-run",
        ])
        .expect("parse deploy dry-run command");

        assert!(matches!(cli.command, Command::Deploy { app, dry_run, .. }
                if app == "myapp" && dry_run));
    }

    #[test]
    fn parses_run_command() {
        let cli = Cli::try_parse_from([
            "v",
            "run",
            "web",
            "--kernel",
            "/kernels/vmlinux",
            "--rootfs",
            "/images/web.ext4",
            "--worker",
            "vps-prod",
            "--tap",
            "tap-web",
            "--vcpu",
            "2",
            "--memory-mib",
            "512",
        ])
        .expect("parse run command");

        assert!(
            matches!(cli.command, Command::Run { app, worker, kernel, rootfs, tap, vcpu, memory_mib, dry_run, .. }
                if app == "web"
                    && worker.as_deref() == Some("vps-prod")
                    && kernel == Path::new("/kernels/vmlinux")
                    && rootfs == Path::new("/images/web.ext4")
                    && tap == "tap-web"
                    && vcpu == 2
                    && memory_mib == 512
                    && !dry_run)
        );
    }

    #[test]
    fn parses_check_command() {
        let cli = Cli::try_parse_from(["v", "check", "web", "--worker", "vps-prod"])
            .expect("parse check command");

        assert!(matches!(cli.command, Command::Check {
                app,
                worker,
                kernel,
                rootfs,
                tap,
                skip_runtime,
                skip_capabilities,
                skip_kernel,
                skip_rootfs,
                skip_tap,
                ..
            }
                if app == "web"
                    && worker.as_deref() == Some("vps-prod")
                    && kernel.is_none()
                    && rootfs.is_none()
                    && tap == "tap0"
                    && !skip_runtime
                    && !skip_capabilities
                    && !skip_kernel
                    && !skip_rootfs
                    && !skip_tap));
    }

    #[test]
    fn parses_check_command_with_files() {
        let cli = Cli::try_parse_from([
            "v",
            "check",
            "web",
            "--worker",
            "vps-prod",
            "--kernel",
            "/kernels/vmlinux",
            "--rootfs",
            "$XDG_DATA_HOME/v/images/web.ext4",
            "--tap",
            "tap-web",
            "--skip-runtime",
            "--skip-capabilities",
            "--skip-kernel",
            "--skip-rootfs",
            "--skip-tap",
        ])
        .expect("parse check command");

        assert!(matches!(cli.command, Command::Check {
                app,
                worker,
                kernel,
                rootfs,
                tap,
                skip_runtime,
                skip_capabilities,
                skip_kernel,
                skip_rootfs,
                skip_tap,
                ..
            }
                if app == "web"
                    && worker.as_deref() == Some("vps-prod")
                    && kernel.as_deref() == Some(Path::new("/kernels/vmlinux"))
                    && rootfs.as_deref() == Some(Path::new("$XDG_DATA_HOME/v/images/web.ext4"))
                    && tap == "tap-web"
                    && skip_runtime
                    && skip_capabilities
                    && skip_kernel
                    && skip_rootfs
                    && skip_tap));
    }

    #[test]
    fn rejects_missing_subcommand() {
        let result = Cli::try_parse_from(["v"]);

        assert!(result.is_err());
    }

    #[test]
    fn parses_setup_command() {
        let cli = Cli::try_parse_from(["v", "setup", "--worker", "vps-prod"])
            .expect("parse setup command");

        assert!(matches!(cli.command, Command::Setup {
                worker, caddy,
            }
                if worker.as_deref() == Some("vps-prod")
                    && !caddy));
    }

    #[test]
    fn parses_setup_with_caddy() {
        let cli = Cli::try_parse_from(["v", "setup", "--caddy"]).expect("parse setup with caddy");

        assert!(matches!(cli.command, Command::Setup { caddy, .. }
                if caddy));
    }
}

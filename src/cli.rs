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
    /// Initialize local host directories and config.
    Init,
    /// Build a local Firecracker boot plan for an app.
    Run {
        app: String,
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
    /// Deploy an immutable app image.
    Deploy { app: String },
    /// Roll back an app to the previous image.
    Rollback { app: String },
    /// List managed microVMs.
    Ps,
    /// Stop an app microVM.
    Stop { app: String },
    /// Show app logs.
    Logs { app: String },
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
        let cli = Cli::try_parse_from(["v", "deploy", "myapp"]).expect("parse deploy command");

        assert!(matches!(cli.command, Command::Deploy { app } if app == "myapp"));
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
            "--tap",
            "tap-web",
            "--vcpu",
            "2",
            "--memory-mib",
            "512",
        ])
        .expect("parse run command");

        assert!(
            matches!(cli.command, Command::Run { app, kernel, rootfs, tap, vcpu, memory_mib, dry_run, .. }
                if app == "web"
                    && kernel == Path::new("/kernels/vmlinux")
                    && rootfs == Path::new("/images/web.ext4")
                    && tap == "tap-web"
                    && vcpu == 2
                    && memory_mib == 512
                    && !dry_run)
        );
    }

    #[test]
    fn rejects_missing_subcommand() {
        let result = Cli::try_parse_from(["v"]);

        assert!(result.is_err());
    }
}

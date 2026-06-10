use clap::{Parser, Subcommand};
use std::path::PathBuf;

use crate::validation::PublishType;

#[derive(Debug, Parser)]
#[command(name = "tamaya")]
#[command(about = "Deploy single-binary apps to a systemd VPS")]
pub struct Cli {
    /// Use this directory as the project root (search for .tamaya.toml from here).
    #[arg(long, value_hint = clap::ValueHint::DirPath)]
    pub project_dir: Option<PathBuf>,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Install and configure worker prerequisites.
    Setup {
        #[arg(long)]
        worker: Option<String>,
    },
    /// Check worker readiness.
    Check {
        #[arg(long)]
        worker: Option<String>,
    },
    /// Deploy a Linux executable.
    Deploy {
        app: Option<String>,
        #[arg(long)]
        worker: Option<String>,
        #[arg(long)]
        binary: Option<PathBuf>,
        #[arg(long)]
        domain: Option<String>,
        #[arg(long)]
        path: Option<String>,
        #[arg(long)]
        dry_run: bool,
        /// Run `ldd` on the worker after upload to verify shared libraries.
        #[arg(long)]
        verify_binary_deps: bool,
    },
    /// Publish a static or SPA site.
    Publish {
        app: Option<String>,
        #[arg(long)]
        worker: Option<String>,
        #[arg(long)]
        path: Option<String>,
        #[arg(long, value_enum)]
        publish_type: Option<PublishType>,
        #[arg(long, value_hint = clap::ValueHint::DirPath)]
        static_root: Option<PathBuf>,
    },
    /// Switch traffic back to the previous successful release.
    Rollback { app: Option<String> },
    /// Show managed app status.
    Status { app: Option<String> },
    /// Stop an app and remove its public route.
    Stop { app: Option<String> },
    /// Delete an app. Shared data is kept unless --purge is passed.
    Delete {
        app: Option<String>,
        #[arg(long)]
        purge: bool,
    },
    /// Stream logs for the current release.
    Logs { app: Option<String> },
    /// Replace the public route with a maintenance response.
    Maintenance {
        app: Option<String>,
        #[arg(long)]
        domain: Option<String>,
        #[arg(long)]
        message: Option<String>,
    },
    /// Restore the public route to the current release.
    Live {
        app: Option<String>,
        #[arg(long)]
        domain: Option<String>,
    },
    /// Manage environment variables for an app.
    Env {
        app: Option<String>,
        #[command(subcommand)]
        action: EnvAction,
    },
    /// Print version information.
    Version,
}

#[derive(Debug, Subcommand)]
pub enum EnvAction {
    Set {
        key: String,
        /// Read the value from standard input instead of prompting.
        #[arg(long)]
        stdin: bool,
    },
    Unset {
        key: String,
    },
    List,
}

impl Command {
    pub fn name(&self) -> &str {
        match self {
            Self::Setup { .. } => "setup",
            Self::Check { .. } => "check",
            Self::Deploy { .. } => "deploy",
            Self::Publish { .. } => "publish",
            Self::Rollback { .. } => "rollback",
            Self::Status { .. } => "status",
            Self::Stop { .. } => "stop",
            Self::Delete { .. } => "delete",
            Self::Logs { .. } => "logs",
            Self::Maintenance { .. } => "maintenance",
            Self::Live { .. } => "live",
            Self::Env { .. } => "env",
            Self::Version => "version",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_deploy() {
        let cli = Cli::try_parse_from([
            "tamaya", "deploy", "web", "--binary", "./web", "--domain", "web.test",
        ])
        .unwrap();
        assert!(
            matches!(cli.command, Command::Deploy { app, binary, domain, path, dry_run: false, .. }
            if app.as_deref() == Some("web")
                && binary == Some(PathBuf::from("./web"))
                && domain.as_deref() == Some("web.test")
                && path.is_none())
        );
    }

    #[test]
    fn parses_deploy_path_override() {
        let cli = Cli::try_parse_from(["tamaya", "deploy", "api", "--path", "/api/"]).unwrap();
        assert!(matches!(cli.command, Command::Deploy { app, path, .. }
            if app.as_deref() == Some("api") && path.as_deref() == Some("/api/")));
    }

    #[test]
    fn parses_publish() {
        let cli = Cli::try_parse_from([
            "tamaya",
            "publish",
            "docs",
            "--path",
            "/docs",
            "--publish-type",
            "spa",
            "--static-root",
            "./dist",
        ])
        .unwrap();
        assert!(
            matches!(cli.command, Command::Publish { app, path, publish_type, static_root, .. }
            if app.as_deref() == Some("docs")
                && path.as_deref() == Some("/docs")
                && publish_type == Some(PublishType::Spa)
                && static_root == Some(PathBuf::from("./dist")))
        );
    }

    #[test]
    fn parses_domain_maintenance_and_live() {
        let maintenance =
            Cli::try_parse_from(["tamaya", "maintenance", "--domain", "example.com"]).unwrap();
        assert!(
            matches!(maintenance.command, Command::Maintenance { app: None, domain, .. }
            if domain.as_deref() == Some("example.com"))
        );

        let live = Cli::try_parse_from(["tamaya", "live", "--domain", "example.com"]).unwrap();
        assert!(matches!(live.command, Command::Live { app: None, domain }
            if domain.as_deref() == Some("example.com")));
    }

    #[test]
    fn parses_delete_purge_and_rollback() {
        let delete = Cli::try_parse_from(["tamaya", "delete", "web", "--purge"]).unwrap();
        assert!(
            matches!(delete.command, Command::Delete { app, purge: true } if app.as_deref() == Some("web"))
        );
        let rollback = Cli::try_parse_from(["tamaya", "rollback", "web"]).unwrap();
        assert!(
            matches!(rollback.command, Command::Rollback { app } if app.as_deref() == Some("web"))
        );
    }

    #[test]
    fn parses_env_set_from_stdin() {
        let cli = Cli::try_parse_from(["tamaya", "env", "web", "set", "TOKEN", "--stdin"]).unwrap();
        assert!(
            matches!(cli.command, Command::Env { app, action: EnvAction::Set { key, stdin: true } }
            if app.as_deref() == Some("web") && key == "TOKEN")
        );
    }

    #[test]
    fn parses_env_list_without_app() {
        let cli = Cli::try_parse_from(["tamaya", "env", "list"]).unwrap();
        assert!(
            matches!(cli.command, Command::Env { app, action: EnvAction::List } if app.is_none())
        );
    }

    #[test]
    fn parses_env_set_without_app() {
        let cli = Cli::try_parse_from(["tamaya", "env", "set", "TOKEN", "--stdin"]).unwrap();
        assert!(
            matches!(cli.command, Command::Env { app, action: EnvAction::Set { key, stdin: true } }
            if app.is_none() && key == "TOKEN")
        );
    }
}

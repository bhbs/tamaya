use clap::{Parser, Subcommand};

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
    fn rejects_missing_subcommand() {
        let result = Cli::try_parse_from(["v"]);

        assert!(result.is_err());
    }
}

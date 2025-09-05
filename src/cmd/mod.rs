mod config;
mod disk_usage;
mod home;
mod list;

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use clap::Args;
use clap::error::ErrorKind as ArgsErrorKind;
use clap::{Parser, Subcommand};

use crate::config::Config;
use crate::config::context::ConfigContext;
use crate::debug;
use crate::exec::{SilentExit, bash, fzf, git};
use crate::term::{confirm, output};

#[async_trait]
pub trait Command {
    async fn run(self) -> Result<()>;
}

#[derive(Parser)]
#[command(author, about, version = env!("ROXIDE_VERSION"))]
pub struct App {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    Config(config::ConfigCommand),
    #[command(alias = "du")]
    DiskUsage(disk_usage::DiskUsageCommand),
    Home(home::HomeCommand),
}

#[async_trait]
impl Command for App {
    async fn run(self) -> Result<()> {
        match self.command {
            Commands::Config(cmd) => cmd.run().await,
            Commands::DiskUsage(cmd) => cmd.run().await,
            Commands::Home(cmd) => cmd.run().await,
        }
    }
}

#[derive(Debug, Args)]
pub struct ConfigArgs {
    /// The config path to use, default is `$HOME/.config/roxide`.
    #[arg(long)]
    pub config_path: Option<String>,

    #[arg(long)]
    pub debug: Option<String>,

    #[arg(long, short)]
    pub yes: bool,
}

impl ConfigArgs {
    pub fn build_ctx(&self) -> Result<Arc<ConfigContext>> {
        ConfigContext::new(self.build_config()?)
    }

    pub fn build_config(&self) -> Result<Config> {
        if let Some(ref file) = self.debug {
            output::set_debug(file.clone());
        }
        let cfg = Config::read(self.config_path.as_deref())?;
        if let Some(ref fzf) = cfg.fzf {
            fzf::set_cmd(fzf.clone());
        }
        if let Some(ref git) = cfg.git {
            git::set_cmd(git.clone());
        }
        if let Some(ref bash) = cfg.bash {
            bash::set_cmd(bash.clone());
        }
        if self.yes {
            confirm::set_no_confirm(true);
        }
        Ok(cfg)
    }
}

#[derive(Debug, Default)]
pub struct CommandResult {
    pub code: u8,
    pub message: Option<String>,
}

pub async fn run() -> CommandResult {
    let app = match App::try_parse() {
        Ok(app) => app,
        Err(err) => {
            err.use_stderr();
            err.print().expect("write help message to stderr");
            if matches!(
                err.kind(),
                ArgsErrorKind::DisplayHelp
                    | ArgsErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
                    | ArgsErrorKind::DisplayVersion
            ) {
                return CommandResult::default();
            }
            return CommandResult {
                code: 2,
                message: None,
            };
        }
    };

    let result = app.run().await;
    let result = match result {
        Ok(()) => CommandResult::default(),
        Err(err) => match err.downcast::<SilentExit>() {
            Ok(SilentExit { code }) => CommandResult {
                code,
                message: None,
            },
            Err(e) => {
                let message = format!("Error: {e:#}");
                CommandResult {
                    code: 1,
                    message: Some(message),
                }
            }
        },
    };
    debug!("[cmd] Cmd result: {result:?}");
    result
}

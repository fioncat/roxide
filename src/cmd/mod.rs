mod attach;
mod check;
mod complete;
mod config;
mod create;
mod detach;
mod disk_usage;
mod display;
mod home;
mod list;
mod open;
mod rebase;
mod remove;
mod squash;
mod switch;
mod sync;

use std::io;
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
pub trait Command: Args {
    async fn run(self) -> Result<()>;

    fn complete_command() -> clap::Command;
}

#[derive(Parser)]
#[command(author, about, version = env!("ROXIDE_VERSION"))]
pub struct App {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    Attach(attach::AttachCommand),
    Check(check::CheckCommand),
    Config(config::ConfigCommand),
    Create(create::CreateCommand),
    Detach(detach::DetachCommand),
    #[command(alias = "du")]
    DiskUsage(disk_usage::DiskUsageCommand),
    Display(display::DisplayCommand),
    Home(home::HomeCommand),
    #[command(alias = "ls")]
    List(list::ListCommand),
    Rebase(rebase::RebaseCommand),
    #[command(alias = "rm")]
    Remove(remove::RemoveCommand),
    Squash(squash::SquashCommand),
    Switch(switch::SwitchCommand),
    Sync(sync::SyncCommand),
}

#[async_trait]
impl Command for App {
    async fn run(self) -> Result<()> {
        match self.command {
            Commands::Attach(cmd) => cmd.run().await,
            Commands::Check(cmd) => cmd.run().await,
            Commands::Config(cmd) => cmd.run().await,
            Commands::Create(cmd) => cmd.run().await,
            Commands::Detach(cmd) => cmd.run().await,
            Commands::DiskUsage(cmd) => cmd.run().await,
            Commands::Display(cmd) => cmd.run().await,
            Commands::Home(cmd) => cmd.run().await,
            Commands::List(cmd) => cmd.run().await,
            Commands::Rebase(cmd) => cmd.run().await,
            Commands::Remove(cmd) => cmd.run().await,
            Commands::Squash(cmd) => cmd.run().await,
            Commands::Switch(cmd) => cmd.run().await,
            Commands::Sync(cmd) => cmd.run().await,
        }
    }

    fn complete_command() -> clap::Command {
        clap::Command::new("rox")
            .disable_help_flag(true)
            .disable_help_subcommand(true)
            .disable_version_flag(true)
            .subcommands([
                attach::AttachCommand::complete_command(),
                check::CheckCommand::complete_command(),
                config::ConfigCommand::complete_command(),
                create::CreateCommand::complete_command(),
                detach::DetachCommand::complete_command(),
                disk_usage::DiskUsageCommand::complete_command(),
                display::DisplayCommand::complete_command(),
                home::HomeCommand::complete_command(),
                list::ListCommand::complete_command(),
                rebase::RebaseCommand::complete_command(),
                remove::RemoveCommand::complete_command(),
                squash::SquashCommand::complete_command(),
                switch::SwitchCommand::complete_command(),
                sync::SyncCommand::complete_command(),
            ])
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

    #[arg(long)]
    pub no_style: bool,
}

impl ConfigArgs {
    pub fn build_ctx(&self) -> Result<Arc<ConfigContext>> {
        ConfigContext::new(self.build_config()?)
    }

    pub fn build_config(&self) -> Result<Config> {
        if let Some(ref file) = self.debug {
            output::set_debug(file.clone());
        }
        if self.yes {
            confirm::set_no_confirm(true);
        }
        if self.no_style {
            output::set_no_style(true);
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
        Ok(cfg)
    }
}

#[derive(Debug, Default)]
pub struct CommandResult {
    pub code: u8,
    pub message: Option<String>,
}

pub async fn run() -> CommandResult {
    match complete::register_complete() {
        Ok(true) => return CommandResult::default(),
        Ok(false) => {}
        Err(e) => {
            return CommandResult {
                code: 1,
                message: Some(format!("Failed to generate init script: {e:#}")),
            };
        }
    }

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

    if !termion::is_tty(&io::stderr()) {
        // We don't allow stderr been redirected, this will cause message been dismissed.
        // Another reason we do this check is that the terminal control characters will be
        // printed in stderr, redirecting it to non-tty will cause confusion.
        // The embed commands are special conditions, their output will be captured by other
        // programs so we should skip this check.
        return CommandResult {
            code: 3,
            message: None,
        };
    }
    // We only print styled message in stderr, so it is safe to enable colors forcibly
    console::set_colors_enabled(true);

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

mod attach;
mod check;
mod complete;
mod config;
mod create;
mod detach;
mod disk_usage;
mod home;
mod list;
mod open;
mod rebase;
mod remove;
mod squash;
mod stats;
mod switch;
mod sync;
mod which;

use std::path::Path;
use std::{env, io};

use anyhow::Result;
use async_trait::async_trait;
use clap::Args;
use clap::error::ErrorKind as ArgsErrorKind;
use clap::{Parser, Subcommand};

use crate::config::context::ConfigContext;
use crate::exec::SilentExit;
use crate::{debug, warn};

#[async_trait]
pub trait Command: Args {
    async fn run(self, ctx: ConfigContext) -> Result<()>;

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
    Home(home::HomeCommand),
    #[command(alias = "ls")]
    List(list::ListCommand),
    Open(open::OpenCommand),
    Rebase(rebase::RebaseCommand),
    #[command(alias = "rm")]
    Remove(remove::RemoveCommand),
    Squash(squash::SquashCommand),
    Stats(stats::StatsCommand),
    Switch(switch::SwitchCommand),
    Sync(sync::SyncCommand),
    Which(which::WhichCommand),
}

#[async_trait]
impl Command for App {
    async fn run(self, ctx: ConfigContext) -> Result<()> {
        match self.command {
            Commands::Attach(cmd) => cmd.run(ctx).await,
            Commands::Check(cmd) => cmd.run(ctx).await,
            Commands::Config(cmd) => cmd.run(ctx).await,
            Commands::Create(cmd) => cmd.run(ctx).await,
            Commands::Detach(cmd) => cmd.run(ctx).await,
            Commands::DiskUsage(cmd) => cmd.run(ctx).await,
            Commands::Home(cmd) => cmd.run(ctx).await,
            Commands::List(cmd) => cmd.run(ctx).await,
            Commands::Open(cmd) => cmd.run(ctx).await,
            Commands::Rebase(cmd) => cmd.run(ctx).await,
            Commands::Remove(cmd) => cmd.run(ctx).await,
            Commands::Squash(cmd) => cmd.run(ctx).await,
            Commands::Stats(cmd) => cmd.run(ctx).await,
            Commands::Switch(cmd) => cmd.run(ctx).await,
            Commands::Sync(cmd) => cmd.run(ctx).await,
            Commands::Which(cmd) => cmd.run(ctx).await,
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
                home::HomeCommand::complete_command(),
                list::ListCommand::complete_command(),
                open::OpenCommand::complete_command(),
                rebase::RebaseCommand::complete_command(),
                remove::RemoveCommand::complete_command(),
                squash::SquashCommand::complete_command(),
                stats::StatsCommand::complete_command(),
                switch::SwitchCommand::complete_command(),
                sync::SyncCommand::complete_command(),
                which::WhichCommand::complete_command(),
            ])
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

    if env::var("ROXIDE_WRAP").is_err() {
        match get_shell_type() {
            Some(shell) if shell == "zsh" || shell == "bash" => {
                warn!(
                    "Please do not use `roxide` command directly, add `source <(ROXIDE_INIT='{shell}' roxide)` to your profile and use `rox` command instead"
                );
            }
            Some(shell) => {
                warn!("Now we do not support shell {shell:?}, please use `bash` or `zsh` instead")
            }
            None => {
                warn!(
                    "Cannot detect your shell type, please make sure you are using `bash` or `zsh`"
                );
            }
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

    if termion::is_tty(&io::stderr()) {
        // We only print styled message in stderr, so it is safe to enable colors forcibly
        console::set_colors_enabled(true);
    }

    let result = async move || -> Result<()> {
        let ctx = ConfigContext::setup()?;
        app.run(ctx).await
    }()
    .await;
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

fn get_shell_type() -> Option<String> {
    let shell = env::var("SHELL").ok()?;
    let name = Path::new(&shell).file_name()?.to_str()?;
    Some(name.to_string())
}

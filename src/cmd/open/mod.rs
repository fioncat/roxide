mod action;
mod pull_request;
mod repo;

use anyhow::Result;
use async_trait::async_trait;
use clap::{Args, Subcommand};

use crate::config::context::ConfigContext;

use super::Command;

#[derive(Args)]
pub struct OpenCommand {
    #[command(subcommand)]
    pub command: OpenCommands,
}

#[derive(Subcommand)]
pub enum OpenCommands {
    Action(action::OpenActionCommand),
    #[command(alias = "pr")]
    PullRequest(pull_request::OpenPullRequestCommand),
    Repo(repo::OpenRepoCommand),
}

#[async_trait]
impl Command for OpenCommand {
    async fn run(self, ctx: ConfigContext) -> Result<()> {
        match self.command {
            OpenCommands::Action(cmd) => cmd.run(ctx).await,
            OpenCommands::PullRequest(cmd) => cmd.run(ctx).await,
            OpenCommands::Repo(cmd) => cmd.run(ctx).await,
        }
    }

    fn complete_command() -> clap::Command {
        clap::Command::new("open")
            .disable_help_flag(true)
            .disable_version_flag(true)
            .subcommands([
                action::OpenActionCommand::complete_command(),
                pull_request::OpenPullRequestCommand::complete_command(),
                repo::OpenRepoCommand::complete_command(),
            ])
    }
}

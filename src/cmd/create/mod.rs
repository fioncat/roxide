mod branch;
mod pull_request;
mod tag;

use anyhow::Result;
use async_trait::async_trait;
use clap::{Args, Subcommand};

use crate::config::context::ConfigContext;

use super::Command;

/// Create commands.
#[derive(Args)]
pub struct CreateCommand {
    #[command(subcommand)]
    pub command: CreateCommands,
}

#[derive(Subcommand)]
pub enum CreateCommands {
    Branch(branch::CreateBranchCommand),
    #[command(alias = "pr")]
    PullRequest(pull_request::CreatePullRequestCommand),
    Tag(tag::CreateTagCommand),
}

#[async_trait]
impl Command for CreateCommand {
    async fn run(self, ctx: ConfigContext) -> Result<()> {
        match self.command {
            CreateCommands::Branch(cmd) => cmd.run(ctx).await,
            CreateCommands::PullRequest(cmd) => cmd.run(ctx).await,
            CreateCommands::Tag(cmd) => cmd.run(ctx).await,
        }
    }

    fn complete_command() -> clap::Command {
        clap::Command::new("create")
            .disable_help_flag(true)
            .disable_version_flag(true)
            .subcommands([
                branch::CreateBranchCommand::complete_command(),
                pull_request::CreatePullRequestCommand::complete_command(),
                tag::CreateTagCommand::complete_command(),
            ])
    }
}

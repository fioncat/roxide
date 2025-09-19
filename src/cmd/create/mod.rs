mod branch;
mod pull_request;
mod tag;

use anyhow::Result;
use async_trait::async_trait;
use clap::{Args, Subcommand};

use crate::cmd::complete::CompleteCommand;
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
    fn name() -> &'static str {
        "create"
    }

    async fn run(self, ctx: ConfigContext) -> Result<()> {
        match self.command {
            CreateCommands::Branch(cmd) => cmd.run(ctx).await,
            CreateCommands::PullRequest(cmd) => cmd.run(ctx).await,
            CreateCommands::Tag(cmd) => cmd.run(ctx).await,
        }
    }

    fn complete() -> CompleteCommand {
        Self::default_complete()
            .subcommand(branch::CreateBranchCommand::complete())
            .subcommand(pull_request::CreatePullRequestCommand::complete())
            .subcommand(tag::CreateTagCommand::complete())
    }
}

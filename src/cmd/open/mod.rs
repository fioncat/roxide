mod action;
mod job;
mod pull_request;
mod repo;

use anyhow::Result;
use async_trait::async_trait;
use clap::{Args, Subcommand};

use crate::cmd::complete::CompleteCommand;
use crate::config::context::ConfigContext;

use super::Command;

/// Open commands.
#[derive(Args)]
pub struct OpenCommand {
    #[command(subcommand)]
    pub command: OpenCommands,
}

#[derive(Subcommand)]
pub enum OpenCommands {
    Action(action::OpenActionCommand),
    Job(job::OpenJobCommand),
    #[command(alias = "pr")]
    PullRequest(pull_request::OpenPullRequestCommand),
    Repo(repo::OpenRepoCommand),
}

#[async_trait]
impl Command for OpenCommand {
    fn name() -> &'static str {
        "open"
    }

    async fn run(self, ctx: ConfigContext) -> Result<()> {
        match self.command {
            OpenCommands::Action(cmd) => cmd.run(ctx).await,
            OpenCommands::Job(cmd) => cmd.run(ctx).await,
            OpenCommands::PullRequest(cmd) => cmd.run(ctx).await,
            OpenCommands::Repo(cmd) => cmd.run(ctx).await,
        }
    }

    fn complete() -> CompleteCommand {
        Self::default_complete()
            .subcommand(action::OpenActionCommand::complete())
            .subcommand(job::OpenJobCommand::complete())
            .subcommand(pull_request::OpenPullRequestCommand::complete())
            .subcommand(repo::OpenRepoCommand::complete())
    }
}

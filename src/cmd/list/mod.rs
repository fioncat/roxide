mod action;
mod branch;
mod hook_history;
mod job;
mod mirror;
mod owner;
mod pull_request;
mod remote;
mod repo;
mod tag;

use anyhow::Result;
use async_trait::async_trait;
use clap::{Args, Subcommand};

use crate::cmd::complete::{CompleteArg, CompleteCommand};
use crate::config::context::ConfigContext;

use super::Command;

/// List commands (alias: `ls`)
#[derive(Args)]
pub struct ListCommand {
    #[command(subcommand)]
    pub command: ListCommands,
}

#[derive(Subcommand)]
pub enum ListCommands {
    Action(action::ListActionCommand),
    Branch(branch::ListBranchCommand),
    HookHistory(hook_history::ListHookHistoryCommand),
    Job(job::ListJobCommand),
    Mirror(mirror::ListMirrorCommand),
    Owner(owner::ListOwnerCommand),
    #[command(alias = "pr")]
    PullRequest(pull_request::ListPullRequestCommand),
    Remote(remote::ListRemoteCommand),
    Repo(repo::ListRepoCommand),
    Tag(tag::ListTagCommand),
}

#[async_trait]
impl Command for ListCommand {
    fn name() -> &'static str {
        "list"
    }

    fn alias() -> Vec<&'static str> {
        vec!["ls"]
    }

    async fn run(self, ctx: ConfigContext) -> Result<()> {
        match self.command {
            ListCommands::Action(cmd) => cmd.run(ctx).await,
            ListCommands::Branch(cmd) => cmd.run(ctx).await,
            ListCommands::HookHistory(cmd) => cmd.run(ctx).await,
            ListCommands::Job(cmd) => cmd.run(ctx).await,
            ListCommands::Mirror(cmd) => cmd.run(ctx).await,
            ListCommands::Owner(cmd) => cmd.run(ctx).await,
            ListCommands::PullRequest(cmd) => cmd.run(ctx).await,
            ListCommands::Remote(cmd) => cmd.run(ctx).await,
            ListCommands::Repo(cmd) => cmd.run(ctx).await,
            ListCommands::Tag(cmd) => cmd.run(ctx).await,
        }
    }

    fn complete() -> CompleteCommand {
        Self::default_complete()
            .subcommand(action::ListActionCommand::complete())
            .subcommand(branch::ListBranchCommand::complete())
            .subcommand(hook_history::ListHookHistoryCommand::complete())
            .subcommand(job::ListJobCommand::complete())
            .subcommand(mirror::ListMirrorCommand::complete())
            .subcommand(owner::ListOwnerCommand::complete())
            .subcommand(pull_request::ListPullRequestCommand::complete())
            .subcommand(remote::ListRemoteCommand::complete())
            .subcommand(repo::ListRepoCommand::complete())
            .subcommand(tag::ListTagCommand::complete())
    }
}

#[derive(Debug, Args)]
pub struct RepoDiskUsageArgs {
    /// If enabled, calculates and displays current disk usage of repositories (or remotes,
    /// owners) sorted in descending order by size. In this mode, some irrelevant columns
    /// will be hidden. Use this option for convenient disk usage monitoring.
    #[arg(name = "disk-usage", short = 'd')]
    pub enable: bool,
}

impl RepoDiskUsageArgs {
    pub fn complete() -> CompleteArg {
        CompleteArg::new().short('d')
    }
}

mod branch;
mod hook_history;
mod mirror;
mod orphan;
mod repo;
mod tag;

use anyhow::Result;
use async_trait::async_trait;
use clap::{Args, Subcommand};

use crate::cmd::complete::CompleteCommand;
use crate::config::context::ConfigContext;

use super::Command;

/// Remove commands (alias `rm`)
#[derive(Args)]
pub struct RemoveCommand {
    #[command(subcommand)]
    pub command: RemoveCommands,
}

#[derive(Subcommand)]
pub enum RemoveCommands {
    Branch(branch::RemoveBranchCommand),
    HookHistory(hook_history::RemoveHookHistoryCommand),
    Mirror(mirror::RemoveMirrorCommand),
    Orphan(orphan::RemoveOrphanCommand),
    Repo(repo::RemoveRepoCommand),
    Tag(tag::RemoveTagCommand),
}

#[async_trait]
impl Command for RemoveCommand {
    fn name() -> &'static str {
        "remove"
    }

    fn alias() -> Vec<&'static str> {
        vec!["rm"]
    }

    async fn run(self, ctx: ConfigContext) -> Result<()> {
        match self.command {
            RemoveCommands::Branch(cmd) => cmd.run(ctx).await,
            RemoveCommands::HookHistory(cmd) => cmd.run(ctx).await,
            RemoveCommands::Mirror(cmd) => cmd.run(ctx).await,
            RemoveCommands::Orphan(cmd) => cmd.run(ctx).await,
            RemoveCommands::Repo(cmd) => cmd.run(ctx).await,
            RemoveCommands::Tag(cmd) => cmd.run(ctx).await,
        }
    }

    fn complete() -> CompleteCommand {
        Self::default_complete()
            .subcommand(branch::RemoveBranchCommand::complete())
            .subcommand(hook_history::RemoveHookHistoryCommand::complete())
            .subcommand(mirror::RemoveMirrorCommand::complete())
            .subcommand(orphan::RemoveOrphanCommand::complete())
            .subcommand(repo::RemoveRepoCommand::complete())
            .subcommand(tag::RemoveTagCommand::complete())
    }
}

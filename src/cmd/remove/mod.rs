mod branch;
mod repo;
mod tag;

use anyhow::Result;
use async_trait::async_trait;
use clap::{Args, Subcommand};

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
    Repo(repo::RemoveRepoCommand),
    Tag(tag::RemoveTagCommand),
}

#[async_trait]
impl Command for RemoveCommand {
    async fn run(self, ctx: ConfigContext) -> Result<()> {
        match self.command {
            RemoveCommands::Branch(cmd) => cmd.run(ctx).await,
            RemoveCommands::Repo(cmd) => cmd.run(ctx).await,
            RemoveCommands::Tag(cmd) => cmd.run(ctx).await,
        }
    }

    fn complete_command() -> clap::Command {
        clap::Command::new("remove")
            .alias("rm")
            .disable_help_flag(true)
            .disable_version_flag(true)
            .subcommands([
                branch::RemoveBranchCommand::complete_command(),
                repo::RemoveRepoCommand::complete_command(),
                tag::RemoveTagCommand::complete_command(),
            ])
    }
}

mod branch;
mod owner;
mod remote;
mod repo;
mod tag;

use anyhow::Result;
use async_trait::async_trait;
use clap::{Args, Subcommand};

use crate::config::context::ConfigContext;

use super::Command;

#[derive(Args)]
pub struct ListCommand {
    #[command(subcommand)]
    pub command: ListCommands,
}

#[derive(Subcommand)]
pub enum ListCommands {
    Branch(branch::ListBranchCommand),
    Owner(owner::ListOwnerCommand),
    Remote(remote::ListRemoteCommand),
    Repo(repo::ListRepoCommand),
    Tag(tag::ListTagCommand),
}

#[async_trait]
impl Command for ListCommand {
    async fn run(self, ctx: ConfigContext) -> Result<()> {
        match self.command {
            ListCommands::Branch(cmd) => cmd.run(ctx).await,
            ListCommands::Owner(cmd) => cmd.run(ctx).await,
            ListCommands::Remote(cmd) => cmd.run(ctx).await,
            ListCommands::Repo(cmd) => cmd.run(ctx).await,
            ListCommands::Tag(cmd) => cmd.run(ctx).await,
        }
    }

    fn complete_command() -> clap::Command {
        clap::Command::new("list")
            .alias("ls")
            .disable_help_flag(true)
            .disable_version_flag(true)
            .subcommands([
                branch::ListBranchCommand::complete_command(),
                owner::ListOwnerCommand::complete_command(),
                remote::ListRemoteCommand::complete_command(),
                repo::ListRepoCommand::complete_command(),
                tag::ListTagCommand::complete_command(),
            ])
    }
}

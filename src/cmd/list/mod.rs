mod branch;
mod owner;
mod remote;
mod repo;

use anyhow::Result;
use async_trait::async_trait;
use clap::{Args, Subcommand};

use super::Command;

#[derive(Args)]
pub struct ListCommand {
    #[command(subcommand)]
    pub command: ListCommands,
}

#[derive(Subcommand)]
pub enum ListCommands {
    Owner(owner::ListOwnerCommand),
    Remote(remote::ListRemoteCommand),
    Repo(repo::ListRepoCommand),
}

#[async_trait]
impl Command for ListCommand {
    async fn run(self) -> Result<()> {
        match self.command {
            ListCommands::Owner(cmd) => cmd.run().await,
            ListCommands::Remote(cmd) => cmd.run().await,
            ListCommands::Repo(cmd) => cmd.run().await,
        }
    }

    fn complete_command() -> clap::Command {
        clap::Command::new("ls")
            .disable_help_flag(true)
            .disable_version_flag(true)
            .subcommands([
                owner::ListOwnerCommand::complete_command(),
                remote::ListRemoteCommand::complete_command(),
                repo::ListRepoCommand::complete_command(),
            ])
    }
}

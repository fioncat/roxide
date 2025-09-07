mod repo;

use anyhow::Result;
use async_trait::async_trait;
use clap::{Args, Subcommand};

use super::Command;

#[derive(Args)]
pub struct OpenCommand {
    #[command(subcommand)]
    pub command: CreateCommands,
}

#[derive(Subcommand)]
pub enum CreateCommands {
    Repo(repo::OpenRepoCommand),
}

#[async_trait]
impl Command for OpenCommand {
    async fn run(self) -> Result<()> {
        match self.command {
            CreateCommands::Repo(cmd) => cmd.run().await,
        }
    }

    fn complete_command() -> clap::Command {
        clap::Command::new("open")
            .disable_help_flag(true)
            .disable_version_flag(true)
            .subcommands([repo::OpenRepoCommand::complete_command()])
    }
}

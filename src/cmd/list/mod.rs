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
    Repo(repo::ListRepoCommand),
}

#[async_trait]
impl Command for ListCommand {
    async fn run(self) -> Result<()> {
        match self.command {
            ListCommands::Repo(cmd) => cmd.run().await,
        }
    }
}

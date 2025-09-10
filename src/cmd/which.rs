use anyhow::Result;
use async_trait::async_trait;
use clap::Args;

use crate::config::context::ConfigContext;
use crate::db::repo::DisplayLevel;
use crate::debug;
use crate::repo::current::get_current_repo;

use super::Command;

/// Display the name of the current repository.
#[derive(Debug, Args)]
pub struct WhichCommand {}

#[async_trait]
impl Command for WhichCommand {
    async fn run(self, ctx: ConfigContext) -> Result<()> {
        debug!("[cmd] Run which command: {:?}", self);

        let repo = get_current_repo(&ctx)?;

        let remote = ctx.cfg.get_remote(&repo.remote)?;

        let text = match remote.icon {
            Some(ref icon) => format!("{icon} {}", repo.display_name(DisplayLevel::Owner)),
            None => repo.full_name(),
        };

        println!("{text}");
        Ok(())
    }

    fn complete_command() -> clap::Command {
        clap::Command::new("which")
    }
}

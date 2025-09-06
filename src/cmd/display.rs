use anyhow::Result;
use async_trait::async_trait;
use clap::Args;

use crate::db::repo::DisplayLevel;
use crate::debug;
use crate::repo::current::get_current_repo;

use super::{Command, ConfigArgs};

#[derive(Debug, Args)]
pub struct DisplayCommand {
    #[clap(flatten)]
    pub config: ConfigArgs,
}

#[async_trait]
impl Command for DisplayCommand {
    async fn run(self) -> Result<()> {
        debug!("[cmd] Run display command: {:?}", self);
        let ctx = self.config.build_ctx()?;
        ctx.lock()?;

        let repo = get_current_repo(ctx.clone())?;

        let remote = ctx.cfg.get_remote(&repo.remote)?;

        let text = match remote.icon {
            Some(ref icon) => format!("{icon} {}", repo.display_name(DisplayLevel::Owner)),
            None => repo.full_name(),
        };

        println!("{text}");
        Ok(())
    }

    fn complete_command() -> clap::Command {
        Self::augment_args(clap::Command::new("display"))
    }
}

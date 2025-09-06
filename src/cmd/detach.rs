use anyhow::{Result, bail};
use async_trait::async_trait;
use clap::Args;

use crate::repo::current::get_current_repo;
use crate::{debug, info};

use super::{Command, ConfigArgs};

#[derive(Debug, Args)]
pub struct DetachCommand {
    #[clap(flatten)]
    pub config: ConfigArgs,
}

#[async_trait]
impl Command for DetachCommand {
    async fn run(self) -> Result<()> {
        let ctx = self.config.build_ctx()?;
        debug!("[cmd] Run detach command: {:?}", self);
        ctx.lock()?;

        if ctx.current_dir.starts_with(&ctx.cfg.workspace) {
            bail!("cannot detach workspace repo, please use `remove repo` command instead");
        }

        let repo = get_current_repo(ctx.clone())?;
        debug!("[cmd] Detach repo: {repo:?}");

        let db = ctx.get_db()?;
        db.with_transaction(|tx| tx.repo().delete(&repo))?;

        info!(
            "Repository {} was detached from current path",
            repo.full_name()
        );
        Ok(())
    }

    fn complete_command() -> clap::Command {
        Self::augment_args(clap::Command::new("detach"))
    }
}

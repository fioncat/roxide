use anyhow::{Result, bail};
use async_trait::async_trait;
use clap::Args;

use crate::config::context::ConfigContext;
use crate::repo::current::get_current_repo;
use crate::{debug, info};

use super::Command;

/// Detach a non-workspace repository from roxide management.
#[derive(Debug, Args)]
pub struct DetachCommand {}

#[async_trait]
impl Command for DetachCommand {
    fn name() -> &'static str {
        "detach"
    }

    async fn run(self, ctx: ConfigContext) -> Result<()> {
        debug!("[cmd] Running detach command: {:?}", self);
        ctx.lock()?;

        if ctx.current_dir.starts_with(&ctx.cfg.workspace) {
            bail!("cannot detach workspace repo, please use `remove repo` command instead");
        }

        let repo = get_current_repo(&ctx)?;
        debug!("[cmd] Detaching repo: {repo:?}");

        let db = ctx.get_db()?;
        db.with_transaction(|tx| tx.repo().delete(&repo))?;

        info!(
            "Repository {} was detached from current path",
            repo.full_name()
        );
        Ok(())
    }
}

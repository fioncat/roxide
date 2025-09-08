use anyhow::{Result, bail};
use async_trait::async_trait;
use clap::Args;

use crate::cmd::complete;
use crate::config::context::ConfigContext;
use crate::repo::current::get_current_repo_optional;
use crate::repo::ops::RepoOperator;
use crate::repo::select::RepoSelector;
use crate::{debug, info};

use super::Command;

#[derive(Debug, Args)]
pub struct AttachCommand {
    pub head: String,

    pub owner: Option<String>,

    pub name: Option<String>,

    #[arg(long, short)]
    pub force_no_cache: bool,
}

#[async_trait]
impl Command for AttachCommand {
    async fn run(self, ctx: ConfigContext) -> Result<()> {
        debug!("[cmd] Run attach command: {:?}", self);
        ctx.lock()?;

        if ctx.current_dir.starts_with(&ctx.cfg.workspace) {
            bail!("cannot attach repo to workspace, please use `home` command instead");
        }

        if let Some(repo) = get_current_repo_optional(&ctx)? {
            bail!(
                "current path is already attached to {}, please detach first",
                repo.full_name()
            );
        }

        let head = Some(self.head);
        let selector = RepoSelector::new(&ctx, &head, &self.owner, &self.name);
        let mut repo = selector.select_remote(self.force_no_cache).await?;
        let remote = ctx.cfg.get_remote(&repo.remote)?;
        let owner = remote.get_owner(&repo.owner);
        repo.visit(owner);
        let path = ctx.current_dir.clone();
        repo.path = Some(format!("{}", path.display()));
        debug!("[cmd] Attach repo: {repo:?}");

        let db = ctx.get_db()?;
        db.with_transaction(|tx| tx.repo().insert(&repo))?;

        let op = RepoOperator::new(&ctx, remote, owner, &repo, path);
        op.ensure_remote()?;
        op.ensure_user()?;

        info!(
            "Repository {} was attached to current path",
            repo.full_name()
        );
        Ok(())
    }

    fn complete_command() -> clap::Command {
        clap::Command::new("attach").args([complete::remote_arg(), complete::owner_arg()])
    }
}

use anyhow::{Result, bail};
use async_trait::async_trait;
use clap::Args;

use crate::config::context::ConfigContext;
use crate::repo::current::get_current_repo_optional;
use crate::repo::ops::RepoOperator;
use crate::repo::select::{RepoSelector, SelectRepoArgs};
use crate::{debug, info};

use super::{CacheArgs, Command, complete};

/// Attach a non-workspace git repository to roxide management.
#[derive(Debug, Args)]
pub struct AttachCommand {
    #[clap(flatten)]
    pub select_repo: SelectRepoArgs,

    #[clap(flatten)]
    pub cache: CacheArgs,
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

        let selector = RepoSelector::new(&ctx, &self.select_repo);
        let mut repo = selector.select_remote(self.cache.force_no_cache).await?;
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

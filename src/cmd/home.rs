use anyhow::Result;
use async_trait::async_trait;
use clap::Args;

use crate::cmd::{CacheArgs, complete};
use crate::config::context::ConfigContext;
use crate::repo::ops::RepoOperator;
use crate::repo::select::{RepoSelector, SelectRepoArgs};
use crate::{confirm, debug};

use super::Command;

/// Enter a repository. If the repository doesn't exist, create or clone it.
#[derive(Debug, Args)]
pub struct HomeCommand {
    #[clap(flatten)]
    pub select_repo: SelectRepoArgs,

    #[clap(flatten)]
    pub cache: CacheArgs,

    /// When searching owners, only search local repositories without calling remote API.
    /// By default, it attempts to search repositories via remote API.
    #[arg(long, short)]
    pub local: bool,

    /// Add `--depth 1` parameter when cloning repositories for faster cloning of large
    /// repositories. Use this parameter if you only need temporary access to the repository.
    /// Note: recommended for readonly mode only, as features like rebase and squash will not
    /// work with shallow repositories.
    #[arg(long, short)]
    pub thin: bool,
}

#[async_trait]
impl Command for HomeCommand {
    async fn run(self, ctx: ConfigContext) -> Result<()> {
        debug!("[cmd] Run home command: {:?}", self);
        ctx.lock()?;

        let selector = RepoSelector::new(&ctx, &self.select_repo);
        let mut repo = selector
            .select_one(self.cache.force_no_cache, self.local)
            .await?;

        let remote = ctx.cfg.get_remote(&repo.remote)?;
        let owner = remote.get_owner(&repo.owner);
        repo.visit(owner);

        let db = ctx.get_db()?;
        if repo.new_created {
            confirm!("Do you want to create {}", repo.full_name());
            db.with_transaction(|tx| tx.repo().insert(&repo))?;
        } else {
            db.with_transaction(|tx| tx.repo().update(&repo))?;
        };

        let path = repo.get_path(&ctx.cfg.workspace);
        let op = RepoOperator::new(&ctx, remote, owner, &repo, path);
        op.ensure_create(self.thin, None)?;

        debug!("[cmd] Home path: {:?}", op.path().display());
        println!("{}", op.path().display());
        Ok(())
    }

    fn complete_command() -> clap::Command {
        clap::Command::new("home").args(complete::repo_args())
    }
}

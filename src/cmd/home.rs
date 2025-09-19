use std::path::PathBuf;

use anyhow::Result;
use async_trait::async_trait;
use clap::Args;

use crate::cmd::complete::{CompleteArg, CompleteCommand};
use crate::cmd::{CacheArgs, ThinArgs};
use crate::config::context::ConfigContext;
use crate::db::repo::Repository;
use crate::repo::mirror::get_current_mirror;
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
    #[arg(short)]
    pub local: bool,

    #[clap(flatten)]
    pub thin: ThinArgs,
}

#[async_trait]
impl Command for HomeCommand {
    fn name() -> &'static str {
        "home"
    }

    async fn run(self, ctx: ConfigContext) -> Result<()> {
        debug!("[cmd] Running home command: {:?}", self);
        ctx.lock()?;

        let mut repo = match self.get_target(&ctx).await? {
            HomeTarget::Repository(r) => r,
            HomeTarget::Path(path) => {
                println!("{}", path.display());
                return Ok(());
            }
        };

        let remote = ctx.cfg.get_remote(&repo.remote)?;
        let owner = remote.get_owner(&repo.owner);
        repo.visit(owner);

        let db = ctx.get_db()?;
        if repo.new_created {
            confirm!("Do you want to create {}", repo.full_name());
            let id = db.with_transaction(|tx| tx.repo().insert(&repo))?;
            repo.id = id;
        } else {
            db.with_transaction(|tx| tx.repo().update(&repo))?;
        };

        let path = repo.get_path(&ctx.cfg.workspace);
        let op = RepoOperator::new(&ctx, remote, owner, &repo, path);
        let create_result = op.ensure_create(self.thin.enable, None)?;
        op.run_hooks(create_result)?;

        debug!("[cmd] Home path: {:?}", op.path().display());
        println!("{}", op.path().display());
        Ok(())
    }

    fn complete() -> CompleteCommand {
        Self::default_complete()
            .args(SelectRepoArgs::complete())
            .arg(CacheArgs::complete())
            .arg(CompleteArg::new().short('l'))
            .arg(ThinArgs::complete())
    }
}

enum HomeTarget {
    Repository(Repository),
    Path(PathBuf),
}

impl HomeCommand {
    async fn get_target(&self, ctx: &ConfigContext) -> Result<HomeTarget> {
        if self.select_repo.head.is_none()
            && let Some((repo, mirror)) = get_current_mirror(ctx)?
        {
            let mirror_path = mirror.get_path(&ctx.cfg.mirrors_dir);
            debug!(
                "[cmd] Current directory is a mirror: {mirror:?}, repo: {repo:?}, mirror_path: {mirror_path:?}"
            );
            if mirror_path == ctx.current_dir {
                debug!(
                    "[cmd] Current directory is the mirror root, return the upstream repository"
                );
                return Ok(HomeTarget::Repository(repo));
            }
            debug!("[cmd] Current directory is inside the mirror, return the mirror path");
            return Ok(HomeTarget::Path(mirror_path));
        };
        debug!("[cmd] Not in mirror, select repository normally");
        let selector = RepoSelector::new(ctx, &self.select_repo);
        let repo = selector
            .select_one(self.cache.force_no_cache, self.local)
            .await?;
        Ok(HomeTarget::Repository(repo))
    }
}

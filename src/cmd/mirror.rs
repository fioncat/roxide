use anyhow::{Result, bail};
use async_trait::async_trait;
use clap::Args;

use crate::cmd::{ThinArgs, complete};
use crate::config::context::ConfigContext;
use crate::repo::current::get_current_repo;
use crate::repo::mirror::MirrorSelector;
use crate::repo::ops::RepoOperator;
use crate::{confirm, debug};

use super::Command;

/// Enter or create a mirror for current repository.
#[derive(Debug, Args)]
pub struct MirrorCommand {
    /// The mirror name to select, default is the current repository name with the "-mirror"
    /// suffix. All mirrors will be cloned to the `mirrors_dir` configured in the config file.
    /// When a repository is deleted, all mirrors are also deleted.
    /// After you enter the mirror, you can use `rox home` (without any arg) to return to
    /// the repository.
    pub name: Option<String>,

    #[clap(flatten)]
    pub thin: ThinArgs,
}

#[async_trait]
impl Command for MirrorCommand {
    async fn run(self, ctx: ConfigContext) -> Result<()> {
        debug!("[cmd] Run mirror command: {:?}", self);
        ctx.lock()?;

        let repo = get_current_repo(&ctx)?;
        let repo_op = RepoOperator::load(&ctx, &repo)?;
        let Some(clone_url) = repo_op.get_clone_url() else {
            bail!("current repository does not support cloning, cannot be mirrored");
        };

        let selector = MirrorSelector::new(&ctx, &repo);
        let mut mirror = selector.select_one(self.name, true)?;
        mirror.visit();

        let db = ctx.get_db()?;
        if mirror.new_created {
            confirm!("Do you want to create mirror {}", mirror.name);
            db.with_transaction(|tx| tx.mirror().insert(&mirror))?;
        } else {
            db.with_transaction(|tx| tx.mirror().update(&mirror))?;
        }

        let mirror_path = mirror.get_path(&ctx.cfg.mirrors_dir);
        let mirror_repo = mirror.into_repo();
        let mirror_op = RepoOperator::new(
            &ctx,
            repo_op.remote(),
            repo_op.owner(),
            &mirror_repo,
            mirror_path,
        );
        mirror_op.ensure_create(self.thin.enable, Some(clone_url))?;

        debug!("[cmd] Mirror path: {:?}", mirror_op.path().display());
        println!("{}", mirror_op.path().display());
        Ok(())
    }

    fn complete_command() -> clap::Command {
        clap::Command::new("mirror").args([complete::mirror_name_arg()])
    }
}

use anyhow::Result;
use async_trait::async_trait;
use clap::Args;

use crate::cmd::complete;
use crate::repo::ops::RepoOperator;
use crate::repo::select::RepoSelector;
use crate::{confirm, debug};

use super::{Command, ConfigArgs};

#[derive(Debug, Args)]
pub struct HomeCommand {
    pub head: Option<String>,

    pub owner: Option<String>,

    pub name: Option<String>,

    #[arg(long, short)]
    pub force_no_cache: bool,

    #[arg(long, short)]
    pub local: bool,

    #[arg(long, short)]
    pub thin: bool,

    #[clap(flatten)]
    pub config: ConfigArgs,
}

#[async_trait]
impl Command for HomeCommand {
    async fn run(self) -> Result<()> {
        debug!("[cmd] Run home command: {:?}", self);
        let ctx = self.config.build_ctx()?;
        ctx.lock()?;

        let selector = RepoSelector::new(ctx.clone(), &self.head, &self.owner, &self.name);
        let mut repo = selector.select_one(self.force_no_cache, self.local).await?;

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
        let op = RepoOperator::new_static(ctx.as_ref(), remote, owner, &repo, path, false);
        op.ensure_create(self.thin, None)?;

        debug!("[cmd] Home path: {:?}", op.path().display());
        println!("{}", op.path().display());
        Ok(())
    }

    fn complete_command() -> clap::Command {
        clap::Command::new("home").args(complete::repo_args())
    }
}

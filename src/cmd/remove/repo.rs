use anyhow::Result;
use async_trait::async_trait;
use clap::Args;

use crate::cmd::Command;
use crate::cmd::complete;
use crate::config::context::ConfigContext;
use crate::db::repo::Repository;
use crate::repo::ops::RepoOperator;
use crate::repo::select::SelectRepoArgs;
use crate::repo::select::{RepoSelector, SelectManyReposOptions};
use crate::term::confirm::confirm_items;
use crate::{confirm, debug, info};

#[derive(Debug, Args)]
pub struct RemoveRepoCommand {
    #[clap(flatten)]
    pub select_repo: SelectRepoArgs,

    #[arg(long, short)]
    pub recursive: bool,

    #[arg(long, short)]
    pub force: bool,
}
#[async_trait]
impl Command for RemoveRepoCommand {
    async fn run(self, ctx: ConfigContext) -> Result<()> {
        debug!("[cmd] Run remove repo command: {:?}", self);
        ctx.lock()?;

        let selector = RepoSelector::new(&ctx, &self.select_repo);
        if !self.recursive {
            let repo = selector.select_one(false, true).await?;
            confirm!("Are you sure to remove repository {}", repo.full_name());
            return self.remove(&ctx, repo);
        }

        let mut opts = SelectManyReposOptions::default();
        if !self.force {
            opts.pin = Some(false);
        }
        let list = selector.select_many(opts)?;
        if list.items.is_empty() {
            info!("No repo to remove");
            return Ok(());
        }

        let names = list.display_names();
        confirm_items(&names, "remove", "removal", "Repo", "Repos")?;

        for repo in list.items {
            self.remove(&ctx, repo)?;
        }

        Ok(())
    }

    fn complete_command() -> clap::Command {
        clap::Command::new("repo").args(complete::repo_args())
    }
}

impl RemoveRepoCommand {
    fn remove(&self, ctx: &ConfigContext, repo: Repository) -> Result<()> {
        let db = ctx.get_db()?;
        let op = RepoOperator::load(ctx, &repo)?;
        op.remove()?;
        db.with_transaction(|tx| tx.repo().delete(&repo))?;
        info!(
            "Repository {} was removed from disk and database",
            repo.full_name()
        );
        Ok(())
    }
}

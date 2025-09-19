use std::mem::take;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use clap::Args;

use crate::batch::{self, Task};
use crate::cmd::complete::{CompleteArg, CompleteCommand};
use crate::config::context::ConfigContext;
use crate::db::repo::Repository;
use crate::repo::current::get_current_repo_optional;
use crate::repo::ops::{RepoOperator, SyncResult};
use crate::repo::select::{RepoSelector, SelectManyReposOptions, SelectRepoArgs};
use crate::term::confirm::confirm_items;
use crate::{debug, outputln};

use super::Command;

/// Synchronize one or multiple repositories. Include cloning, pushing, pulling, etc.
#[derive(Debug, Args)]
pub struct SyncCommand {
    #[clap(flatten)]
    pub select_repo: SelectRepoArgs,

    /// By default, if you are currently in a repository, sync will synchronize the current
    /// repository; otherwise it will sync multiple repositories. With this option,
    /// multiple repositories will be synced regardless of your current location.
    #[arg(short)]
    pub recursive: bool,

    /// By default, when syncing multiple repositories, only repositories marked with sync
    /// flag will be synchronized. With this option, all repositories will be forcefully
    /// synced, ignoring the sync flag.
    #[arg(short)]
    pub force: bool,
}

#[async_trait]
impl Command for SyncCommand {
    fn name() -> &'static str {
        "sync"
    }

    async fn run(self, ctx: ConfigContext) -> Result<()> {
        debug!("[cmd] Run sync command: {:?}", self);

        if !self.recursive
            && let Some(repo) = get_current_repo_optional(&ctx)?
        {
            return self.sync_one(ctx, repo);
        }
        self.sync_many(ctx).await
    }

    fn complete() -> CompleteCommand {
        Self::default_complete()
            .args(SelectRepoArgs::complete())
            .arg(CompleteArg::new().short('r'))
            .arg(CompleteArg::new().short('f'))
    }
}

impl SyncCommand {
    async fn sync_many(self, mut ctx: ConfigContext) -> Result<()> {
        let selector = RepoSelector::new(&ctx, &self.select_repo);
        let mut opts = SelectManyReposOptions::default();
        if !self.force {
            opts.sync = Some(true);
        }
        let list = selector.select_many(opts)?;
        if list.items.is_empty() {
            outputln!("No repo to sync");
            return Ok(());
        }

        let mut names = list.display_names();
        confirm_items(&names, "Sync", "synchronization", "Repo", "Repos")?;

        let mut tasks = Vec::with_capacity(list.items.len());
        ctx.mute();
        let ctx = Arc::new(ctx);
        for (idx, repo) in list.items.into_iter().enumerate() {
            let task = SyncTask {
                name: Arc::new(take(&mut names[idx])),
                ctx: ctx.clone(),
                repo,
            };
            tasks.push(task);
        }

        let results = batch::run("Syncing", "Sync", tasks).await?;
        let results: Vec<String> = results
            .iter()
            .filter_map(|r| {
                let text = r.render(true);
                if text.is_empty() { None } else { Some(text) }
            })
            .collect();
        outputln!();
        if results.is_empty() {
            outputln!("No result to display");
            return Ok(());
        }

        for (idx, result) in results.iter().enumerate() {
            outputln!("{result}");
            if idx != results.len() - 1 {
                outputln!();
            }
        }

        Ok(())
    }

    fn sync_one(self, ctx: ConfigContext, repo: Repository) -> Result<()> {
        let op = RepoOperator::load(&ctx, &repo)?;
        let result = op.sync()?;
        let text = result.render(false);
        if text.is_empty() {
            outputln!("No result to display");
            return Ok(());
        }

        outputln!();
        outputln!("Result:");
        outputln!("{text}");
        Ok(())
    }
}

struct SyncTask {
    name: Arc<String>,
    ctx: Arc<ConfigContext>,
    repo: Repository,
}

#[async_trait]
impl Task<SyncResult> for SyncTask {
    fn name(&self) -> Arc<String> {
        self.name.clone()
    }

    async fn run(&self) -> Result<SyncResult> {
        RepoOperator::load(self.ctx.as_ref(), &self.repo)?.sync()
    }
}

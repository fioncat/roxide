use anyhow::Result;
use async_trait::async_trait;
use clap::Args;

use crate::cmd::complete::CompleteCommand;
use crate::config::context::ConfigContext;
use crate::repo::current::get_current_repo_optional;
use crate::repo::ops::RepoOperator;
use crate::repo::select::{RepoSelector, SelectManyReposOptions, SelectRepoArgs};
use crate::term::confirm::confirm_items;
use crate::{debug, info};

use super::{Command, RecursiveArgs};

/// Detect language for one or multiple repositories
#[derive(Debug, Args)]
pub struct DetectCommand {
    #[clap(flatten)]
    pub select_repo: SelectRepoArgs,

    #[clap(flatten)]
    pub recursive: RecursiveArgs,
}

#[async_trait]
impl Command for DetectCommand {
    fn name() -> &'static str {
        "detect"
    }

    async fn run(self, ctx: ConfigContext) -> Result<()> {
        debug!("[cmd] Running sync command: {:?}", self);

        if !self.recursive.enable
            && let Some(repo) = get_current_repo_optional(&ctx)?
        {
            info!("Detecting language for current repo");
            let op = RepoOperator::load(&ctx, &repo)?;
            return op.update_language(true).await;
        }

        let selector = RepoSelector::new(&ctx, &self.select_repo);
        let list = selector.select_many(SelectManyReposOptions::default())?;

        let names = list.display_names();
        confirm_items(&names, "detect", "detection", "Repo", "Repos")?;

        let repos = list.items;
        for (idx, repo) in repos.iter().enumerate() {
            info!("Detecting language for: {}", names[idx]);
            let op = RepoOperator::load(&ctx, repo)?;
            op.update_language(true).await?;
        }

        Ok(())
    }

    fn complete() -> CompleteCommand {
        Self::default_complete()
            .args(SelectRepoArgs::complete())
            .arg(RecursiveArgs::complete())
    }
}

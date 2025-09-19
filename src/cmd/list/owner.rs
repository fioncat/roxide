use anyhow::Result;
use async_trait::async_trait;
use clap::Args;

use crate::cmd::Command;
use crate::cmd::complete::{CompleteArg, CompleteCommand, funcs};
use crate::config::context::ConfigContext;
use crate::repo::disk_usage::{OwnerDiskUsage, OwnerDiskUsageList, repo_disk_usage};
use crate::repo::select::SelectRepoArgs;
use crate::repo::select::{RepoSelector, SelectManyReposOptions, select_owners};
use crate::term::list::{ListArgs, pagination};
use crate::{debug, outputln};

use super::RepoDiskUsageArgs;

/// List repository owners.
#[derive(Debug, Args)]
pub struct ListOwnerCommand {
    /// Remote name. If not specified, list owners across all remotes.
    pub remote: Option<String>,

    #[clap(flatten)]
    pub disk_usage: RepoDiskUsageArgs,

    #[clap(flatten)]
    pub list: ListArgs,
}

#[async_trait]
impl Command for ListOwnerCommand {
    fn name() -> &'static str {
        "owner"
    }

    async fn run(self, ctx: ConfigContext) -> Result<()> {
        debug!("[cmd] Running list owner command: {:?}", self);

        let limit = self.list.limit();
        let text = if self.disk_usage.enable {
            let args = SelectRepoArgs {
                head: self.remote,
                ..Default::default()
            };
            let selector = RepoSelector::new(&ctx, &args);
            let repos = selector.select_many(SelectManyReposOptions::default())?;
            let usages = repo_disk_usage(&ctx, repos.items).await?;
            let usages = OwnerDiskUsage::group_by_repo_usages(usages);
            let (usages, total) = pagination(usages, limit);
            let list = OwnerDiskUsageList {
                show_remote: args.head.is_none(),
                usages,
                total,
            };
            self.list.render(list)?
        } else {
            let list = select_owners(&ctx, self.remote, limit)?;
            self.list.render(list)?
        };

        outputln!("{text}");
        Ok(())
    }

    fn complete() -> CompleteCommand {
        Self::default_complete()
            .arg(CompleteArg::new().complete(funcs::complete_remote))
            .arg(RepoDiskUsageArgs::complete())
            .args(ListArgs::complete())
    }
}

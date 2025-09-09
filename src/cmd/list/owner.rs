use anyhow::Result;
use async_trait::async_trait;
use clap::Args;

use crate::cmd::Command;
use crate::cmd::complete;
use crate::config::context::ConfigContext;
use crate::repo::disk_usage::{OwnerDiskUsage, OwnerDiskUsageList, repo_disk_usage};
use crate::repo::select::SelectRepoArgs;
use crate::repo::select::{RepoSelector, SelectManyReposOptions, select_owners};
use crate::term::list::{ListArgs, pagination};
use crate::{debug, outputln};

#[derive(Debug, Args)]
pub struct ListOwnerCommand {
    pub remote: Option<String>,

    #[arg(long = "du", short)]
    pub disk_usage: bool,

    #[clap(flatten)]
    pub list: ListArgs,
}

#[async_trait]
impl Command for ListOwnerCommand {
    async fn run(self, ctx: ConfigContext) -> Result<()> {
        debug!("[cmd] Run list owner command: {:?}", self);

        let limit = self.list.limit();
        let text = if self.disk_usage {
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

    fn complete_command() -> clap::Command {
        clap::Command::new("owner").arg(complete::remote_arg())
    }
}

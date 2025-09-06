use anyhow::Result;
use async_trait::async_trait;
use clap::Args;

use crate::cmd::complete;
use crate::cmd::{Command, ConfigArgs};
use crate::output;
use crate::repo::disk_usage::{OwnerDiskUsage, OwnerDiskUsageList, repo_disk_usage};
use crate::repo::select::{RepoSelector, SelectManyReposOptions, select_owners};
use crate::term::list::{ListArgs, PageArgs, pagination};

#[derive(Debug, Args)]
pub struct ListOwnerCommand {
    pub remote: Option<String>,

    #[arg(long = "du", short)]
    pub disk_usage: bool,

    #[clap(flatten)]
    pub list: ListArgs,

    #[clap(flatten)]
    pub page: PageArgs,

    #[clap(flatten)]
    pub config: ConfigArgs,
}

#[async_trait]
impl Command for ListOwnerCommand {
    async fn run(self) -> Result<()> {
        let ctx = self.config.build_ctx()?;

        let limit = self.page.limit();
        let text = if self.disk_usage {
            let selector = RepoSelector::new(ctx.clone(), &self.remote, &None, &None);
            let repos = selector.select_many(SelectManyReposOptions::default())?;
            let usages = repo_disk_usage(ctx, repos.items).await?;
            let usages = OwnerDiskUsage::group_by_repo_usages(usages);
            let (usages, total) = pagination(usages, limit);
            let list = OwnerDiskUsageList {
                show_remote: self.remote.is_none(),
                usages,
                total,
            };
            self.list.render(list, Some(self.page))?
        } else {
            let list = select_owners(ctx, self.remote, limit)?;
            self.list.render(list, Some(self.page))?
        };

        output!("{text}");
        Ok(())
    }

    fn complete_command() -> clap::Command {
        clap::Command::new("owner").arg(complete::remote_arg())
    }
}

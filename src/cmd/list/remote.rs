use anyhow::Result;
use async_trait::async_trait;
use clap::Args;

use crate::cmd::Command;
use crate::config::context::ConfigContext;
use crate::repo::disk_usage::{RemoteDiskUsage, RemoteDiskUsageList, repo_disk_usage};
use crate::repo::select::{RepoSelector, SelectManyReposOptions, SelectRepoArgs, select_remotes};
use crate::term::list::{ListArgs, pagination};
use crate::{debug, outputln};

#[derive(Debug, Args)]
pub struct ListRemoteCommand {
    #[arg(long = "du", short)]
    pub disk_usage: bool,

    #[clap(flatten)]
    pub list: ListArgs,
}

#[async_trait]
impl Command for ListRemoteCommand {
    async fn run(self, ctx: ConfigContext) -> Result<()> {
        debug!("[cmd] Run list remote command: {:?}", self);

        let limit = self.list.limit();
        let text = if self.disk_usage {
            let args = SelectRepoArgs::default();
            let selector = RepoSelector::new(&ctx, &args);
            let repos = selector.select_many(SelectManyReposOptions::default())?;
            let usages = repo_disk_usage(&ctx, repos.items).await?;
            let usages = RemoteDiskUsage::group_by_repo_usages(usages);
            let (usages, total) = pagination(usages, limit);
            let list = RemoteDiskUsageList { usages, total };
            self.list.render(list)?
        } else {
            let list = select_remotes(&ctx, limit)?;
            self.list.render(list)?
        };
        outputln!("{text}");
        Ok(())
    }

    fn complete_command() -> clap::Command {
        clap::Command::new("remote")
    }
}

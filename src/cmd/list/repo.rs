use anyhow::Result;
use async_trait::async_trait;
use clap::Args;

use crate::cmd::Command;
use crate::cmd::complete::CompleteArg;
use crate::cmd::complete::CompleteCommand;
use crate::config::context::ConfigContext;
use crate::debug;
use crate::outputln;
use crate::repo::disk_usage::{RepoDiskUsageList, repo_disk_usage};
use crate::repo::select::SelectRepoArgs;
use crate::repo::select::{RepoSelector, SelectManyReposOptions};
use crate::term::list::{ListArgs, pagination};

use super::RepoDiskUsageArgs;

/// List repositories.
#[derive(Debug, Args)]
pub struct ListRepoCommand {
    #[clap(flatten)]
    pub select_repo: SelectRepoArgs,

    /// Filter with sync flag.
    #[arg(long)]
    pub sync: bool,

    /// Filter with pin flag.
    #[arg(long)]
    pub pin: bool,

    #[clap(flatten)]
    pub disk_usage: RepoDiskUsageArgs,

    #[clap(flatten)]
    pub list: ListArgs,
}

#[async_trait]
impl Command for ListRepoCommand {
    fn name() -> &'static str {
        "repo"
    }

    async fn run(self, ctx: ConfigContext) -> Result<()> {
        debug!("[cmd] Running list repo command: {:?}", self);

        let selector = RepoSelector::new(&ctx, &self.select_repo);
        let limit = self.list.limit();
        let mut opts = SelectManyReposOptions::default();
        if self.sync {
            opts.sync = Some(true);
        }
        if self.pin {
            opts.pin = Some(true);
        }
        if !self.disk_usage.enable {
            opts.limit = Some(limit);
        }

        let list = selector.select_many(opts)?;
        let text = if self.disk_usage.enable {
            let level = list.level;
            let usages = repo_disk_usage(&ctx, list.items).await?;
            let (usages, total) = pagination(usages, limit);
            let list = RepoDiskUsageList {
                usages,
                total,
                level,
            };
            self.list.render(list)?
        } else {
            self.list.render(list)?
        };

        outputln!("{text}");
        Ok(())
    }

    fn complete() -> CompleteCommand {
        Self::default_complete()
            .args(SelectRepoArgs::complete())
            .arg(CompleteArg::new().long("sync"))
            .arg(CompleteArg::new().long("pin"))
            .arg(RepoDiskUsageArgs::complete())
            .args(ListArgs::complete())
    }
}

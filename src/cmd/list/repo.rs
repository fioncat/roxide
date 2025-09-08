use anyhow::Result;
use async_trait::async_trait;
use clap::Args;

use crate::cmd::Command;
use crate::cmd::complete;
use crate::config::context::ConfigContext;
use crate::repo::disk_usage::{RepoDiskUsageList, repo_disk_usage};
use crate::repo::select::{RepoSelector, SelectManyReposOptions};
use crate::term::list::{ListArgs, pagination};
use crate::{debug, output};

#[derive(Debug, Args)]
pub struct ListRepoCommand {
    pub remote: Option<String>,

    pub owner: Option<String>,

    pub name: Option<String>,

    #[arg(long)]
    pub sync: bool,

    #[arg(long)]
    pub pin: bool,

    #[arg(long = "du", short)]
    pub disk_usage: bool,

    #[clap(flatten)]
    pub list: ListArgs,
}

#[async_trait]
impl Command for ListRepoCommand {
    async fn run(self, ctx: ConfigContext) -> Result<()> {
        debug!("[cmd] Run list repo command: {:?}", self);

        let selector = RepoSelector::new(&ctx, &self.remote, &self.owner, &self.name);
        let limit = self.list.limit();
        let mut opts = SelectManyReposOptions::default();
        if self.sync {
            opts.sync = Some(true);
        }
        if self.pin {
            opts.pin = Some(true);
        }
        if !self.disk_usage {
            opts.limit = Some(limit);
        }

        let list = selector.select_many(opts)?;
        let text = if self.disk_usage {
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

        output!("{text}");
        Ok(())
    }

    fn complete_command() -> clap::Command {
        clap::Command::new("repo").args(complete::repo_args())
    }
}

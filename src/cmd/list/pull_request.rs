use anyhow::Result;
use async_trait::async_trait;
use clap::Args;

use crate::cmd::{CacheArgs, Command, complete};
use crate::config::context::ConfigContext;
use crate::repo::current::get_current_repo;
use crate::repo::select::SelectPullRequestsArgs;
use crate::term::list::TableArgs;
use crate::{debug, outputln};

/// List pull requests.
#[derive(Debug, Args)]
pub struct ListPullRequestCommand {
    #[clap(flatten)]
    pub select_pull_requests: SelectPullRequestsArgs,

    #[clap(flatten)]
    pub cache: CacheArgs,

    #[clap(flatten)]
    pub table: TableArgs,
}

#[async_trait]
impl Command for ListPullRequestCommand {
    async fn run(self, ctx: ConfigContext) -> Result<()> {
        debug!("[cmd] Run list pull request command: {:?}", self);

        let mut titles = vec![];
        if self.select_pull_requests.id.is_some() {
            titles.push("Head");
            titles.push("Base");
        } else {
            titles.push("ID");
            if self.select_pull_requests.all {
                titles.push("Head");
            }
            if self.select_pull_requests.base.is_none() {
                titles.push("Base");
            }
        }
        titles.push("Title");

        let repo = get_current_repo(&ctx)?;
        let prs = self
            .select_pull_requests
            .select_many(&ctx, &repo, self.cache.force_no_cache)
            .await?;
        debug!("[cmd] Pull requests: {prs:?}");

        let text = self.table.render(titles, &prs)?;
        outputln!("{text}");
        Ok(())
    }

    fn complete_command() -> clap::Command {
        clap::Command::new("pull-request")
            .alias("pr")
            .args(complete::list_pull_requests_args())
    }
}

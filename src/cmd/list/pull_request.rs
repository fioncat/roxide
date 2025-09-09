use anyhow::Result;
use async_trait::async_trait;
use clap::Args;

use crate::cmd::Command;
use crate::cmd::complete;
use crate::config::context::ConfigContext;
use crate::repo::select::SelectPullRequestsArgs;
use crate::term::list::TableArgs;
use crate::{debug, outputln};

#[derive(Debug, Args)]
pub struct ListPullRequestCommand {
    #[clap(flatten)]
    pub select_pull_requests: SelectPullRequestsArgs,

    #[arg(long, short)]
    pub force_no_cache: bool,

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

        let prs = self
            .select_pull_requests
            .select_many(&ctx, self.force_no_cache)
            .await?;
        debug!("[cmd] Pull requests: {prs:?}");

        let text = self.table.render(titles, &prs)?;
        outputln!("{text}");
        Ok(())
    }

    fn complete_command() -> clap::Command {
        clap::Command::new("pull-request").args(complete::list_pull_requests_args())
    }
}

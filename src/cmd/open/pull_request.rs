use anyhow::{Context, Result};
use async_trait::async_trait;
use clap::Args;

use crate::cmd::Command;
use crate::cmd::complete;
use crate::config::context::ConfigContext;
use crate::debug;
use crate::repo::select::SelectPullRequestsArgs;
use crate::term::list::TableArgs;

#[derive(Debug, Args)]
pub struct OpenPullRequestCommand {
    #[clap(flatten)]
    pub select_pull_requests: SelectPullRequestsArgs,

    #[arg(long, short)]
    pub force_no_cache: bool,

    #[clap(flatten)]
    pub table: TableArgs,
}

#[async_trait]
impl Command for OpenPullRequestCommand {
    async fn run(self, ctx: ConfigContext) -> Result<()> {
        debug!("[cmd] Run open pull request command: {:?}", self);

        let pr = self
            .select_pull_requests
            .select_one(&ctx, self.force_no_cache, None)
            .await?;
        debug!("[cmd] Pull request: {pr:?}");

        open::that(&pr.web_url)
            .with_context(|| format!("cannot open pull request web url {:?}", pr.web_url))?;
        Ok(())
    }

    fn complete_command() -> clap::Command {
        clap::Command::new("pull-request")
            .alias("pr")
            .args(complete::list_pull_requests_args())
    }
}

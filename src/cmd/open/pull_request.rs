use anyhow::{Context, Result};
use async_trait::async_trait;
use clap::Args;

use crate::cmd::CacheArgs;
use crate::cmd::Command;
use crate::cmd::complete;
use crate::config::context::ConfigContext;
use crate::debug;
use crate::repo::current::get_current_repo;
use crate::repo::select::SelectPullRequestsArgs;
use crate::repo::wait_action::WaitActionArgs;
use crate::term::list::TableArgs;

/// Open current pull request in the browser.
#[derive(Debug, Args)]
pub struct OpenPullRequestCommand {
    #[clap(flatten)]
    pub select_pull_requests: SelectPullRequestsArgs,

    #[clap(flatten)]
    pub cache: CacheArgs,

    #[clap(flatten)]
    pub table: TableArgs,

    #[clap(flatten)]
    pub wait: WaitActionArgs,
}

#[async_trait]
impl Command for OpenPullRequestCommand {
    async fn run(self, ctx: ConfigContext) -> Result<()> {
        debug!("[cmd] Run open pull request command: {:?}", self);

        let repo = get_current_repo(&ctx)?;
        let pr = self
            .select_pull_requests
            .select_one(&ctx, &repo, self.cache.force_no_cache, None)
            .await?;
        debug!("[cmd] Pull request: {pr:?}");

        if self.wait.enable {
            let api = ctx.get_api(&repo.remote, self.cache.force_no_cache)?;
            self.wait.wait(&ctx, &repo, api.as_ref()).await?;
        }

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

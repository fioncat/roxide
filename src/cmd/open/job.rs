use anyhow::{Context, Result};
use async_trait::async_trait;
use clap::Args;

use crate::cmd::Command;
use crate::cmd::complete::{CompleteArg, CompleteCommand};
use crate::config::context::ConfigContext;
use crate::debug;
use crate::repo::current::get_current_repo;
use crate::repo::select::select_job_from_action;
use crate::repo::wait_action::WaitActionArgs;
use crate::term::list::TableArgs;

/// Open a job in the browser.
#[derive(Debug, Args)]
pub struct OpenJobCommand {
    /// Job ID to open. If not specified, select one from current action.
    pub id: Option<u64>,

    #[clap(flatten)]
    pub wait_action: WaitActionArgs,

    #[clap(flatten)]
    pub table: TableArgs,
}

#[async_trait]
impl Command for OpenJobCommand {
    fn name() -> &'static str {
        "job"
    }

    async fn run(self, ctx: ConfigContext) -> Result<()> {
        debug!("[cmd] Run open job command: {:?}", self);

        let repo = get_current_repo(&ctx)?;
        let api = ctx.get_api(&repo.remote, false)?;
        let action = self.wait_action.wait(&ctx, &repo, api.as_ref()).await?;
        let job = select_job_from_action(&ctx, action, self.id, None)?;
        open::that(&job.web_url).with_context(|| format!("failed to open job: {}", job.web_url))
    }

    fn complete() -> CompleteCommand {
        Self::default_complete()
            .arg(CompleteArg::new().no_complete_value())
            .arg(WaitActionArgs::complete())
            .args(TableArgs::complete())
    }
}

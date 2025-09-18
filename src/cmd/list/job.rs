use anyhow::Result;
use async_trait::async_trait;
use clap::Args;

use crate::cmd::Command;
use crate::config::context::ConfigContext;
use crate::debug;
use crate::repo::current::get_current_repo;
use crate::repo::select::select_job_from_action;
use crate::repo::wait_action::WaitActionArgs;
use crate::term::list::TableArgs;

/// Print logs for a specific job.
#[derive(Debug, Args)]
pub struct ListJobCommand {
    /// Job ID to get logs for. If not specified, select one from current action.
    pub id: Option<u64>,

    #[clap(flatten)]
    pub wait_action: WaitActionArgs,

    #[clap(flatten)]
    pub table: TableArgs,
}

#[async_trait]
impl Command for ListJobCommand {
    fn name() -> &'static str {
        "job"
    }

    async fn run(self, ctx: ConfigContext) -> Result<()> {
        debug!("[cmd] Run list job command: {:?}", self);

        let repo = get_current_repo(&ctx)?;
        let api = ctx.get_api(&repo.remote, false)?;

        if let Some(id) = self.id {
            let logs = api.get_job_log(&repo.owner, &repo.name, id).await?;
            print!("{logs}");
            return Ok(());
        }

        let action = self.wait_action.wait(&ctx, &repo, api.as_ref()).await?;

        let job = select_job_from_action(&ctx, action, None, None)?;
        let logs = api.get_job_log(&repo.owner, &repo.name, job.id).await?;
        print!("{logs}");
        Ok(())
    }
}

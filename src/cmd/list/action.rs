use anyhow::Result;
use async_trait::async_trait;
use clap::Args;
use console::style;

use crate::cmd::Command;
use crate::config::context::ConfigContext;
use crate::repo::current::get_current_repo;
use crate::repo::wait_action::WaitActionArgs;
use crate::term::list::TableArgs;
use crate::{debug, outputln};

/// List actions and jobs (CI/CD) information for the current commit.
#[derive(Debug, Args)]
pub struct ListActionCommand {
    #[clap(flatten)]
    pub wait_action: WaitActionArgs,

    #[clap(flatten)]
    pub table: TableArgs,
}

#[async_trait]
impl Command for ListActionCommand {
    fn name() -> &'static str {
        "action"
    }

    async fn run(self, mut ctx: ConfigContext) -> Result<()> {
        debug!("[cmd] Run list action command: {:?}", self);
        ctx.mute();

        let repo = get_current_repo(&ctx)?;
        let api = ctx.get_api(&repo.remote, false)?;

        let action = self.wait_action.wait(&ctx, &repo, api.as_ref()).await?;

        if self.table.json {
            let json = serde_json::to_string_pretty(&action)?;
            outputln!("{json}");
            return Ok(());
        }

        outputln!("CommitID: {}", action.commit_id);
        outputln!("Message:  {}", action.commit_message);
        outputln!("User:     {}", action.user);
        outputln!("Email:    {}", action.email);
        outputln!();

        let groups_count = action.job_groups.len();
        for (idx, group) in action.job_groups.into_iter().enumerate() {
            outputln!("{}", style(format!("[{}]", group.name)).magenta().bold());
            let text = self
                .table
                .render(vec!["ID", "Name", "Status"], &group.jobs)?;
            outputln!("{text}");

            if idx + 1 != groups_count {
                outputln!();
            }
        }

        Ok(())
    }
}

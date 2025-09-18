use anyhow::{Context, Result};
use async_trait::async_trait;
use clap::Args;

use crate::cmd::Command;
use crate::cmd::complete::CompleteCommand;
use crate::config::context::ConfigContext;
use crate::debug;
use crate::repo::current::get_current_repo;
use crate::repo::wait_action::WaitActionArgs;
use crate::term::list::TableArgs;

/// Open current action in the browser.
#[derive(Debug, Args)]
pub struct OpenActionCommand {
    #[clap(flatten)]
    pub wait_action: WaitActionArgs,

    #[clap(flatten)]
    pub table: TableArgs,
}

#[async_trait]
impl Command for OpenActionCommand {
    fn name() -> &'static str {
        "action"
    }

    async fn run(self, mut ctx: ConfigContext) -> Result<()> {
        debug!("[cmd] Run open action command: {:?}", self);
        ctx.mute();

        let repo = get_current_repo(&ctx)?;
        let api = ctx.get_api(&repo.remote, false)?;

        let action = self.wait_action.wait(&ctx, &repo, api.as_ref()).await?;

        open::that(&action.web_url)
            .with_context(|| format!("failed to open action: {}", action.web_url))
    }

    fn complete() -> CompleteCommand {
        Self::default_complete()
            .arg(WaitActionArgs::complete())
            .args(TableArgs::complete())
    }
}

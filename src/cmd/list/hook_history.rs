use anyhow::Result;
use async_trait::async_trait;
use clap::Args;

use crate::config::context::ConfigContext;
use crate::repo::current::get_current_repo;
use crate::term::list::TableArgs;
use crate::{debug, outputln};

use super::Command;

/// List hook histories for current repository.
#[derive(Debug, Args)]
pub struct ListHookHistoryCommand {
    #[clap(flatten)]
    pub table: TableArgs,
}

#[async_trait]
impl Command for ListHookHistoryCommand {
    fn name() -> &'static str {
        "hook-history"
    }

    async fn run(self, ctx: ConfigContext) -> Result<()> {
        debug!("[cmd] Run list hook_history command: {:?}", self);

        let repo = get_current_repo(&ctx)?;
        let db = ctx.get_db()?;
        let histories = db.with_transaction(|tx| tx.hook_history().query_by_repo_id(repo.id))?;
        let text = self
            .table
            .render(vec!["Name", "Success", "Time"], &histories)?;
        outputln!("{text}");
        Ok(())
    }
}

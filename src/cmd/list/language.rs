use anyhow::Result;
use async_trait::async_trait;
use clap::Args;

use crate::cmd::Command;
use crate::cmd::complete::CompleteCommand;
use crate::config::context::ConfigContext;
use crate::term::list::TableArgs;
use crate::{debug, outputln};

/// List all languages
#[derive(Debug, Args)]
pub struct ListLanguageCommand {
    #[clap(flatten)]
    pub table: TableArgs,
}

#[async_trait]
impl Command for ListLanguageCommand {
    fn name() -> &'static str {
        "language"
    }

    async fn run(self, ctx: ConfigContext) -> Result<()> {
        debug!("[cmd] Running list language command: {:?}", self);

        let db = ctx.get_db()?;
        let languages = db.with_transaction(|tx| tx.repo().query_languages())?;

        let text = self.table.render(vec!["Language", "Count"], &languages)?;
        outputln!("{text}");
        Ok(())
    }

    fn complete() -> CompleteCommand {
        Self::default_complete().args(TableArgs::complete())
    }
}

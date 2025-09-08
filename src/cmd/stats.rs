use std::env;
use std::path::PathBuf;

use anyhow::{Context, Result};
use async_trait::async_trait;
use clap::Args;

use crate::config::context::ConfigContext;
use crate::scan::code_stats::get_code_stats;
use crate::scan::ignore::Ignore;
use crate::term::list::TableArgs;
use crate::{debug, output, report_scan_process_done};

use super::Command;

#[derive(Debug, Args)]
pub struct StatsCommand {
    pub path: Option<String>,

    #[arg(long, short = 'I')]
    pub ignore: Option<Vec<String>>,

    #[clap(flatten)]
    pub table: TableArgs,
}

#[async_trait]
impl Command for StatsCommand {
    async fn run(self, _: ConfigContext) -> Result<()> {
        debug!("[cmd] Run stats command: {:?}", self);

        let path = match self.path {
            Some(path) => PathBuf::from(path),
            None => env::current_dir().context("failed to get current directory")?,
        };
        let ignore = match self.ignore {
            Some(ignores) => Ignore::parse(&path, &ignores)?,
            None => Ignore::default(),
        };

        let stats = get_code_stats(path, ignore).await?;
        report_scan_process_done!(stats);

        let text = self.table.render(
            vec!["Lang", "Files", "Code", "Comment", "Blank", "Total"],
            &stats.items,
        )?;
        output!("{text}");
        Ok(())
    }

    fn complete_command() -> clap::Command {
        Self::augment_args(clap::Command::new("stats"))
    }
}

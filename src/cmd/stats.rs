use std::env;
use std::path::PathBuf;

use anyhow::{Context, Result};
use async_trait::async_trait;
use clap::Args;

use crate::config::context::ConfigContext;
use crate::scan::code_stats::get_code_stats;
use crate::scan::ignore::Ignore;
use crate::term::list::TableArgs;
use crate::{debug, outputln, report_scan_process_done};

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
    async fn run(self, ctx: ConfigContext) -> Result<()> {
        debug!("[cmd] Run stats command: {:?}", self);

        let path = match self.path {
            Some(path) => PathBuf::from(path),
            None => env::current_dir().context("failed to get current directory")?,
        };
        let mut ignore_patterns = self.ignore.unwrap_or_default();
        ignore_patterns.extend(ctx.cfg.stats_ignore);

        let ignore = if ignore_patterns.is_empty() {
            Ignore::default()
        } else {
            Ignore::parse(&path, &ignore_patterns)?
        };

        let stats = get_code_stats(path, ignore).await?;
        report_scan_process_done!(stats);

        let text = self.table.render(
            vec!["Lang", "Files", "Code", "Comment", "Blank", "Total"],
            &stats.items,
        )?;
        outputln!("{text}");
        Ok(())
    }

    fn complete_command() -> clap::Command {
        Self::augment_args(clap::Command::new("stats"))
    }
}

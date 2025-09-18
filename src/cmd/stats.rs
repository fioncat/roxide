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

use super::{Command, IgnoreArgs};

/// Quickly calculate lines of code for different languages in the repository.
#[derive(Debug, Args)]
pub struct StatsCommand {
    /// Directory path to calculate statistics for. If not provided, calculates code in the
    /// current directory.
    pub path: Option<String>,

    #[clap(flatten)]
    pub ignore: IgnoreArgs,

    #[clap(flatten)]
    pub table: TableArgs,
}

#[async_trait]
impl Command for StatsCommand {
    fn name() -> &'static str {
        "stats"
    }

    async fn run(self, ctx: ConfigContext) -> Result<()> {
        debug!("[cmd] Run stats command: {:?}", self);

        let path = match self.path {
            Some(path) => PathBuf::from(path),
            None => env::current_dir().context("failed to get current directory")?,
        };
        let mut ignore_patterns = self.ignore.patterns.unwrap_or_default();
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
}

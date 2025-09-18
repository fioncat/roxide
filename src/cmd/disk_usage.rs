use std::env;
use std::path::PathBuf;

use anyhow::{Context, Result};
use async_trait::async_trait;
use clap::Args;

use crate::config::context::ConfigContext;
use crate::scan::disk_usage::disk_usage;
use crate::scan::ignore::Ignore;
use crate::{debug, outputln, report_scan_process_done};

use super::Command;

/// Calculate disk usage statistics for a directory (alias `du`)
#[derive(Debug, Args)]
pub struct DiskUsageCommand {
    /// Specify the directory to calculate statistics for.
    pub path: Option<String>,

    /// Display directory hierarchy levels.
    #[arg(long, short, default_value = "1")]
    pub depth: usize,

    /// Ignore certain files or directories during calculation.
    #[arg(long, short = 'I')]
    pub ignore: Option<Vec<String>>,

    /// Display execution time and speed of the calculation.
    #[arg(long, short)]
    pub stats: bool,
}

#[async_trait]
impl Command for DiskUsageCommand {
    fn name() -> &'static str {
        "disk-usage"
    }

    async fn run(self, _: ConfigContext) -> Result<()> {
        debug!("[cmd] Run disk-usage command: {:?}", self);

        let path = match self.path {
            Some(path) => PathBuf::from(path),
            None => env::current_dir().context("failed to get current directory")?,
        };
        let ignore = match self.ignore {
            Some(ignores) => Ignore::parse(&path, &ignores)?,
            None => Ignore::default(),
        };

        let usage = disk_usage(path, self.depth, ignore).await?;
        if self.stats {
            report_scan_process_done!(usage);
        }

        let items = usage.formatted();
        for (path, usage) in items {
            outputln!("{usage}  {}", path.display());
        }

        Ok(())
    }
}

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

#[derive(Debug, Args)]
pub struct DiskUsageCommand {
    pub path: Option<String>,

    #[arg(long, short, default_value = "1")]
    pub depth: usize,

    #[arg(long, short = 'I')]
    pub ignore: Option<Vec<String>>,

    #[arg(long, short)]
    pub stats: bool,
}

#[async_trait]
impl Command for DiskUsageCommand {
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

    fn complete_command() -> clap::Command {
        Self::augment_args(clap::Command::new("disk-usage"))
    }
}

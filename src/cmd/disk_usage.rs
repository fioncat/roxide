use std::env;
use std::path::PathBuf;

use anyhow::{Context, Result};
use async_trait::async_trait;
use clap::Args;

use crate::format::format_elapsed;
use crate::outputln;
use crate::scan::disk_usage::disk_usage;
use crate::scan::ignore::Ignore;

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
    async fn run(self) -> Result<()> {
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
            outputln!(
                "Total {} files, took: {}, speed: {} files/s",
                usage.files_count,
                format_elapsed(usage.elapsed),
                usage.speed
            );
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

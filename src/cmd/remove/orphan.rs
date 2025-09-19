use anyhow::Result;
use async_trait::async_trait;
use clap::Args;
use console::style;

use crate::cmd::Command;
use crate::cmd::complete::{CompleteArg, CompleteCommand};
use crate::config::context::ConfigContext;
use crate::outputln;
use crate::repo::remove_dir_all;
use crate::repo::scan_orphans::scan_orphans;
use crate::{confirm, debug, info};

/// Remove orphan directories and files in workspace and mirrors.
#[derive(Debug, Args)]
pub struct RemoveOrphanCommand {
    /// Show what would be removed without actually removing them.
    #[arg(short)]
    pub dry_run: bool,
}

#[async_trait]
impl Command for RemoveOrphanCommand {
    fn name() -> &'static str {
        "orphan"
    }

    async fn run(self, ctx: ConfigContext) -> Result<()> {
        debug!("[cmd] Running remove orphan command: {:?}", self);

        let orphans = scan_orphans(&ctx).await?;

        if !orphans.workspace.is_empty() {
            outputln!(
                "{}",
                style("Orphan path(s) in workspace (Relative)")
                    .blue()
                    .bold()
            );
            for path in orphans.workspace.iter() {
                let rel_path = path.strip_prefix(&ctx.cfg.workspace).unwrap_or(path);
                outputln!("* {}", rel_path.display());
            }
        }
        if !orphans.mirrors.is_empty() {
            if !orphans.workspace.is_empty() {
                outputln!();
            }
            outputln!(
                "{}",
                style("Orphan path(s) in mirrors (Relative)").blue().bold()
            );
            for path in orphans.mirrors.iter() {
                let rel_path = path.strip_prefix(&ctx.cfg.mirrors_dir).unwrap_or(path);
                outputln!("* {}", rel_path.display());
            }
        }

        if orphans.workspace.is_empty() && orphans.mirrors.is_empty() {
            info!("No orphan path found");
            return Ok(());
        }

        outputln!();
        if self.dry_run {
            info!("Dry run mode, nothing is removed");
            return Ok(());
        }

        confirm!("Continue to remove");

        let mut count = 0;
        for path in orphans.workspace.iter() {
            count += 1;
            remove_dir_all(&ctx, path)?;
        }
        for path in orphans.mirrors.iter() {
            count += 1;
            remove_dir_all(&ctx, path)?;
        }

        info!("Done, {count} orphan path(s) removed");
        Ok(())
    }

    fn complete() -> CompleteCommand {
        Self::default_complete().arg(CompleteArg::new().short('d'))
    }
}

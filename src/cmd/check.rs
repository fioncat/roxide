use anyhow::Result;
use async_trait::async_trait;
use clap::Args;
use console::style;

use crate::check::*;
use crate::cmd::CacheArgs;
use crate::cmd::complete::CompleteCommand;
use crate::config::context::ConfigContext;
use crate::db::repo::{DisplayLevel, QueryOptions};
use crate::{cursor_up, debug, outputln};

use super::Command;

/// Check system environment and verify roxide functionality.
#[derive(Debug, Args)]
pub struct CheckCommand {
    #[clap(flatten)]
    pub cache: CacheArgs,
}

#[async_trait]
impl Command for CheckCommand {
    fn name() -> &'static str {
        "check"
    }

    async fn run(self, ctx: ConfigContext) -> Result<()> {
        debug!("[cmd] Run check command: {:?}", self);

        show_section("Environment");
        check_os().show();
        check_shell().show();
        outputln!();

        show_section("Commands");
        check_git(&ctx).show();
        check_fzf(&ctx).show();
        check_bash(&ctx).show();
        check_edit(&ctx).show();
        outputln!();

        show_section("Remote API");
        let db = ctx.get_db()?;
        let remotes = db.with_transaction(|tx| tx.repo().query_remotes(None))?;
        if remotes.is_empty() {
            outputln!("No remote to check");
            return Ok(());
        }
        for remote in remotes {
            outputln!("Checking remote {} ...", remote.remote);
            let (item, ok) = check_remote(&ctx, &remote.remote, self.cache.force_no_cache).await;
            cursor_up!();
            item.show();
            if !ok {
                continue;
            }

            let repos = db.with_transaction(|tx| {
                tx.repo().query(QueryOptions {
                    remote: Some(&remote.remote),
                    ..Default::default()
                })
            })?;

            for repo in repos {
                outputln!("- Checking {} ...", repo.display_name(DisplayLevel::Owner));
                let item = check_repo(&ctx, &repo, self.cache.force_no_cache).await;
                cursor_up!();
                item.show();
            }
        }

        Ok(())
    }

    fn complete() -> CompleteCommand {
        Self::default_complete().arg(CacheArgs::complete())
    }
}

fn show_section(section: &str) {
    outputln!("{}", style(format!("[{section}]")).bold());
}

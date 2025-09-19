use std::collections::HashSet;

use anyhow::Result;
use async_trait::async_trait;
use clap::Args;

use crate::cmd::complete::{CompleteArg, CompleteCommand};
use crate::config::context::ConfigContext;
use crate::db::DatabaseHandle;
use crate::repo::current::get_current_repo;
use crate::{debug, info};

use super::Command;

/// Remove hook histories for current repository.
#[derive(Debug, Args)]
pub struct RemoveHookHistoryCommand {
    /// Remove all hook histories, not only for the current repository.
    #[arg(short)]
    pub all: bool,

    /// Remove orphan hook histories, which do not exists in hooks config or belong to any
    /// repository.
    #[arg(short)]
    pub orphan: bool,
}

#[async_trait]
impl Command for RemoveHookHistoryCommand {
    fn name() -> &'static str {
        "hook-history"
    }

    async fn run(self, ctx: ConfigContext) -> Result<()> {
        debug!("[cmd] Run remove hook_history command: {:?}", self);

        let db = ctx.get_db()?;
        if self.all || self.orphan {
            return db.with_transaction(|tx| self.remove_many(&ctx, tx));
        }

        let repo = get_current_repo(&ctx)?;
        db.with_transaction(|tx| tx.hook_history().delete_by_repo_id(repo.id))?;
        Ok(())
    }
}

impl RemoveHookHistoryCommand {
    fn remove_many(&self, ctx: &ConfigContext, tx: &DatabaseHandle) -> Result<()> {
        let current_hooks = ctx
            .cfg
            .hooks
            .iter()
            .map(|h| h.name.as_str())
            .collect::<HashSet<_>>();
        let items = tx.hook_history().query_all()?;
        if items.is_empty() {
            info!("No hook history found");
            return Ok(());
        }
        for item in items {
            let should_remove = if self.all {
                info!("Removing hook history {}", item.id);
                true
            } else if !current_hooks.contains(item.name.as_str()) {
                info!(
                    "Removing hook history {}, hook {:?} not exists in config",
                    item.id, item.name
                );
                true
            } else if tx.repo().get_by_id(item.repo_id)?.is_none() {
                info!(
                    "Removing hook history {}, repo id {} not exists",
                    item.id, item.repo_id
                );
                true
            } else {
                false
            };
            if should_remove {
                tx.hook_history().delete(item.id)?;
            }
        }
        Ok(())
    }

    fn complete() -> CompleteCommand {
        Self::default_complete()
            .arg(CompleteArg::new().short('a'))
            .arg(CompleteArg::new().short('o'))
    }
}

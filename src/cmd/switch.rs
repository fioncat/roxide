use anyhow::{Result, bail};
use async_trait::async_trait;
use clap::Args;

use crate::cmd::complete::{CompleteArg, CompleteCommand, funcs};
use crate::config::context::ConfigContext;
use crate::debug;
use crate::exec::git::branch::Branch;
use crate::exec::git::commit::ensure_no_uncommitted_changes;

use super::Command;

/// Switch to a different Git branch.
#[derive(Debug, Args)]
pub struct SwitchCommand {
    /// Branch name to switch to. If not provided, searches and selects from all branches
    /// (including remote branches).
    pub branch: Option<String>,
}

#[async_trait]
impl Command for SwitchCommand {
    fn name() -> &'static str {
        "switch"
    }

    async fn run(self, ctx: ConfigContext) -> Result<()> {
        debug!("[cmd] Run switch command: {:?}", self);

        ensure_no_uncommitted_changes(ctx.git())?;

        if let Some(ref branch) = self.branch {
            return self.switch(&ctx, branch);
        }

        let current_branch = Branch::current(ctx.git())?;
        let branches = Branch::list_remote(ctx.git())?;
        debug!("[cmd] Current branch: {current_branch:?}, remote branches: {branches:?}");

        let branches = branches
            .into_iter()
            .filter(|b| b != &current_branch)
            .collect::<Vec<_>>();
        if branches.is_empty() {
            bail!("no branch to switch");
        }

        if branches.len() == 1 {
            debug!("[cmd] Only one branch to switch: {:?}", branches[0]);
            return self.switch(&ctx, &branches[0]);
        }

        let idx = ctx.fzf_search("Search branch to switch", &branches, None)?;
        debug!("[cmd] Selected branch: {:?}", branches[idx]);
        self.switch(&ctx, &branches[idx])
    }

    fn complete() -> CompleteCommand {
        Self::default_complete().arg(CompleteArg::new().complete(funcs::complete_branch))
    }
}

impl SwitchCommand {
    #[inline]
    fn switch(&self, ctx: &ConfigContext, branch: &str) -> Result<()> {
        ctx.git()
            .execute(["checkout", branch], format!("Switch to branch {branch:?}"))
    }
}

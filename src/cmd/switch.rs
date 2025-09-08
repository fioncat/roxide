use anyhow::{Result, bail};
use async_trait::async_trait;
use clap::Args;

use crate::cmd::complete;
use crate::config::context::ConfigContext;
use crate::debug;
use crate::exec::fzf;
use crate::exec::git::branch::Branch;
use crate::exec::git::commit::ensure_no_uncommitted_changes;

use super::Command;

#[derive(Debug, Args)]
pub struct SwitchCommand {
    pub branch: Option<String>,
}

#[async_trait]
impl Command for SwitchCommand {
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

        let idx = fzf::search("Search branch to switch", &branches, None)?;
        debug!("[cmd] Selected branch: {:?}", branches[idx]);
        self.switch(&ctx, &branches[idx])
    }

    fn complete_command() -> clap::Command {
        clap::Command::new("switch").arg(complete::branch_arg())
    }
}

impl SwitchCommand {
    #[inline]
    fn switch(&self, ctx: &ConfigContext, branch: &str) -> Result<()> {
        ctx.git()
            .execute(["checkout", branch], format!("Switch to branch {branch:?}"))
    }
}

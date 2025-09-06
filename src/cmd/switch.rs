use anyhow::{Result, bail};
use async_trait::async_trait;
use clap::Args;

use crate::cmd::complete;
use crate::debug;
use crate::exec::git::branch::Branch;
use crate::exec::git::commit::ensure_no_uncommitted_changes;
use crate::exec::{fzf, git};

use super::{Command, ConfigArgs};

#[derive(Debug, Args)]
pub struct SwitchCommand {
    pub branch: Option<String>,

    #[clap(flatten)]
    pub config: ConfigArgs,
}

#[async_trait]
impl Command for SwitchCommand {
    async fn run(self) -> Result<()> {
        self.config.build_ctx()?;
        debug!("[cmd] Run switch command: {:?}", self);

        ensure_no_uncommitted_changes(None::<&str>, false)?;

        if let Some(ref branch) = self.branch {
            return self.switch(branch);
        }

        let current_branch = Branch::current(None::<&str>, false)?;
        let branches = Branch::list_remote(None::<&str>, false)?;
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
            return self.switch(&branches[0]);
        }

        let idx = fzf::search("Search branch to switch", &branches, None)?;
        debug!("[cmd] Selected branch: {:?}", branches[idx]);
        self.switch(&branches[idx])
    }

    fn complete_command() -> clap::Command {
        clap::Command::new("switch").arg(complete::branch_arg())
    }
}

impl SwitchCommand {
    #[inline]
    fn switch(&self, branch: &str) -> Result<()> {
        git::new(
            ["checkout", branch],
            None::<&str>,
            format!("Switch to branch {branch:?}"),
            false,
        )
        .execute()
    }
}

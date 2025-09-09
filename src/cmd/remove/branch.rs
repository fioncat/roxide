use anyhow::{Result, bail};
use async_trait::async_trait;
use clap::Args;

use crate::cmd::Command;
use crate::cmd::complete;
use crate::config::context::ConfigContext;
use crate::debug;
use crate::exec::git::branch::{Branch, BranchStatus};
use crate::exec::git::commit::ensure_no_uncommitted_changes;

#[derive(Debug, Args)]
pub struct RemoveBranchCommand {
    pub branch: Option<String>,
}

#[async_trait]
impl Command for RemoveBranchCommand {
    async fn run(self, ctx: ConfigContext) -> Result<()> {
        debug!("[cmd] Run remove branch command: {:?}", self);

        let branches = Branch::list(ctx.git())?;
        let default_branch = Branch::default(ctx.git())?;
        let branch = match self.branch {
            Some(branch) => {
                let found = branches.iter().any(|b| b.name == branch.as_str());
                if !found {
                    bail!("cannot find branch {branch:?}");
                }
                if branch == default_branch {
                    bail!("cannot remove default branch {default_branch:?}");
                }
                branch
            }
            None => {
                let current = Branch::current(ctx.git())?;
                if current == default_branch {
                    bail!("you are in the default branch {default_branch:?}, cannot remove it");
                }
                ensure_no_uncommitted_changes(ctx.git())?;
                ctx.git().execute(
                    ["checkout", &default_branch],
                    format!("Checkout to default branch {default_branch:?}"),
                )?;
                current
            }
        };

        ctx.git().execute(
            ["branch", "-D", &branch],
            format!("Remove local branch {branch:?}"),
        )?;

        let branch = branches.into_iter().find(|b| b.name == branch);
        if let Some(branch) = branch
            && !matches!(branch.status, BranchStatus::Gone | BranchStatus::Detached)
        {
            ctx.git().execute(
                ["push", "origin", "--delete", &branch.name],
                format!("Remove remote branch {:?}", branch.name),
            )?;
        }

        Ok(())
    }

    fn complete_command() -> clap::Command {
        clap::Command::new("branch").arg(complete::branch_arg())
    }
}

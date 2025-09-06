use anyhow::{Result, bail};
use async_trait::async_trait;
use clap::Args;

use crate::cmd::complete;
use crate::cmd::{Command, ConfigArgs};
use crate::debug;
use crate::exec::git;
use crate::exec::git::branch::{Branch, BranchStatus};
use crate::exec::git::commit::ensure_no_uncommitted_changes;

#[derive(Debug, Args)]
pub struct RemoveBranchCommand {
    pub branch: Option<String>,

    #[clap(flatten)]
    pub config: ConfigArgs,
}

#[async_trait]
impl Command for RemoveBranchCommand {
    async fn run(self) -> Result<()> {
        self.config.build_ctx()?;
        debug!("[cmd] Run remove branch command: {:?}", self);

        let branches = Branch::list(None::<&str>, false)?;
        let default_branch = Branch::default(None::<&str>, false)?;
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
                let current = Branch::current(None::<&str>, false)?;
                if current == default_branch {
                    bail!("you are in the default branch {default_branch:?}, cannot remove it");
                }
                ensure_no_uncommitted_changes(None::<&str>, false)?;
                git::new(
                    ["checkout", &default_branch],
                    None::<&str>,
                    format!("Checkout to default branch {default_branch:?}"),
                    false,
                )
                .execute()?;
                current
            }
        };

        git::new(
            ["branch", "-D", &branch],
            None::<&str>,
            format!("Remove local branch {branch:?}"),
            false,
        )
        .execute()?;

        let branch = branches.into_iter().find(|b| b.name == branch);
        if let Some(branch) = branch
            && !matches!(branch.status, BranchStatus::Gone | BranchStatus::Detached)
        {
            git::new(
                ["push", "origin", "--delete", &branch.name],
                None::<&str>,
                format!("Remove remote branch {branch:?}"),
                false,
            )
            .execute()?;
        }

        Ok(())
    }

    fn complete_command() -> clap::Command {
        clap::Command::new("branch").arg(complete::branch_arg())
    }
}

use anyhow::Result;
use async_trait::async_trait;
use clap::Args;

use crate::cmd::Command;
use crate::config::context::ConfigContext;
use crate::exec::git::branch::Branch;
use crate::{debug, output};

/// Create a new branch and push it to the remote.
#[derive(Debug, Args)]
pub struct CreateBranchCommand {
    /// Name of the branch to create.
    pub branch: String,
}

#[async_trait]
impl Command for CreateBranchCommand {
    fn name() -> &'static str {
        "branch"
    }

    async fn run(self, ctx: ConfigContext) -> Result<()> {
        debug!("[cmd] Running create branch command: {:?}", self);

        let branches = Branch::list(ctx.git())?;
        for branch in branches {
            if branch.name == self.branch {
                output!("Branch {:?} already exists", self.branch);
                return Ok(());
            }
        }

        ctx.git().execute(
            ["checkout", "-b", &self.branch],
            format!("Creating branch {}", self.branch),
        )?;

        ctx.git().execute(
            ["push", "--set-upstream", "origin", &self.branch],
            format!("Pushing branch {} to remote", self.branch),
        )?;

        Ok(())
    }
}

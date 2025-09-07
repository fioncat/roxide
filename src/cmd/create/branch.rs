use anyhow::Result;
use async_trait::async_trait;
use clap::Args;

use crate::cmd::{Command, ConfigArgs};
use crate::exec::git;
use crate::exec::git::branch::Branch;
use crate::{debug, output};

#[derive(Debug, Args)]
pub struct CreateBranchCommand {
    pub branch: String,

    #[clap(flatten)]
    pub config: ConfigArgs,
}

#[async_trait]
impl Command for CreateBranchCommand {
    async fn run(self) -> Result<()> {
        self.config.build_ctx()?;
        debug!("[cmd] Run create branch command: {:?}", self);

        let branches = Branch::list(None::<&str>, true)?;
        for branch in branches {
            if branch.name == self.branch {
                output!("Branch {:?} already exists", self.branch);
                return Ok(());
            }
        }

        git::new(
            ["checkout", "-b", &self.branch],
            None::<&str>,
            format!("Create branch {}", self.branch),
            false,
        )
        .execute()?;

        git::new(
            ["push", "--set-upstream", "origin", &self.branch],
            None::<&str>,
            format!("Push branch {} to remote", self.branch),
            true,
        )
        .execute()?;

        Ok(())
    }

    fn complete_command() -> clap::Command {
        clap::Command::new("branch")
    }
}

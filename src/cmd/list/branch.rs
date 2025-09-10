use anyhow::Result;
use async_trait::async_trait;
use clap::Args;

use crate::cmd::Command;
use crate::config::context::ConfigContext;
use crate::exec::git::branch::{Branch, BranchList};
use crate::term::list::{ListArgs, pagination};
use crate::{debug, outputln};

/// List git branches.
#[derive(Debug, Args)]
pub struct ListBranchCommand {
    #[clap(flatten)]
    pub list: ListArgs,
}

#[async_trait]
impl Command for ListBranchCommand {
    async fn run(self, mut ctx: ConfigContext) -> Result<()> {
        debug!("[cmd] Run list branch command: {:?}", self);
        ctx.mute();

        let branches = Branch::list(ctx.git())?;
        let (branches, total) = pagination(branches, self.list.limit());

        let list = BranchList { branches, total };
        let text = self.list.render(list)?;

        outputln!("{text}");
        Ok(())
    }

    fn complete_command() -> clap::Command {
        clap::Command::new("branch")
    }
}

use anyhow::Result;
use async_trait::async_trait;
use clap::Args;

use crate::cmd::{Command, ConfigArgs};
use crate::exec::git::branch::{Branch, BranchList};
use crate::term::list::{ListArgs, pagination};
use crate::{debug, output};

#[derive(Debug, Args)]
pub struct ListBranchCommand {
    #[clap(flatten)]
    pub list: ListArgs,

    #[clap(flatten)]
    pub config: ConfigArgs,
}

#[async_trait]
impl Command for ListBranchCommand {
    async fn run(self) -> Result<()> {
        self.config.build_ctx()?;
        debug!("[cmd] Run list branch command: {:?}", self);

        let branches = Branch::list(None::<&str>, true)?;
        let (branches, total) = pagination(branches, self.list.limit());

        let list = BranchList { branches, total };
        let text = self.list.render(list)?;

        output!("{text}");
        Ok(())
    }

    fn complete_command() -> clap::Command {
        Self::augment_args(clap::Command::new("branch"))
    }
}

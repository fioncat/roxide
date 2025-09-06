use anyhow::Result;
use async_trait::async_trait;
use clap::Args;

use crate::cmd::{Command, ConfigArgs};
use crate::exec::git::tag::{Tag, TagList};
use crate::term::list::{ListArgs, pagination};
use crate::{debug, output};

#[derive(Debug, Args)]
pub struct ListTagCommand {
    #[clap(flatten)]
    pub list: ListArgs,

    #[clap(flatten)]
    pub config: ConfigArgs,
}

#[async_trait]
impl Command for ListTagCommand {
    async fn run(self) -> Result<()> {
        self.config.build_ctx()?;
        debug!("[cmd] Run list tag command: {:?}", self);

        let tags = Tag::list(None::<&str>, true)?;
        let (tags, total) = pagination(tags, self.list.limit());

        let list = TagList { tags, total };
        let text = self.list.render(list)?;

        output!("{text}");
        Ok(())
    }

    fn complete_command() -> clap::Command {
        Self::augment_args(clap::Command::new("tag"))
    }
}

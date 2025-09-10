use anyhow::Result;
use async_trait::async_trait;
use clap::Args;

use crate::cmd::Command;
use crate::config::context::ConfigContext;
use crate::exec::git::tag::{Tag, TagList};
use crate::term::list::{ListArgs, pagination};
use crate::{debug, outputln};

/// List git tags.
#[derive(Debug, Args)]
pub struct ListTagCommand {
    #[clap(flatten)]
    pub list: ListArgs,
}

#[async_trait]
impl Command for ListTagCommand {
    async fn run(self, mut ctx: ConfigContext) -> Result<()> {
        debug!("[cmd] Run list tag command: {:?}", self);
        ctx.mute();

        let tags = Tag::list(ctx.git())?;
        let (tags, total) = pagination(tags, self.list.limit());

        let list = TagList { tags, total };
        let text = self.list.render(list)?;

        outputln!("{text}");
        Ok(())
    }

    fn complete_command() -> clap::Command {
        Self::augment_args(clap::Command::new("tag"))
    }
}

use anyhow::Result;
use async_trait::async_trait;
use clap::Args;

use crate::cmd::Command;
use crate::cmd::complete::CompleteCommand;
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
    fn name() -> &'static str {
        "tag"
    }

    async fn run(self, mut ctx: ConfigContext) -> Result<()> {
        debug!("[cmd] Running list tag command: {:?}", self);
        ctx.mute();

        let tags = Tag::list(ctx.git())?;
        let (tags, total) = pagination(tags, self.list.limit());

        let list = TagList { tags, total };
        let text = self.list.render(list)?;

        outputln!("{text}");
        Ok(())
    }

    fn complete() -> CompleteCommand {
        Self::default_complete().args(ListArgs::complete())
    }
}

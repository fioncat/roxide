use std::mem::take;

use anyhow::Result;
use async_trait::async_trait;
use clap::Args;

use crate::cmd::Command;
use crate::cmd::complete;
use crate::config::context::ConfigContext;
use crate::debug;
use crate::exec::git::tag::Tag;

#[derive(Debug, Args)]
pub struct RemoveTagCommand {
    pub tag: Option<String>,
}

#[async_trait]
impl Command for RemoveTagCommand {
    async fn run(self, ctx: ConfigContext) -> Result<()> {
        debug!("[cmd] Run remove tag command: {:?}", self);

        let tag = match self.tag {
            Some(ref tag) => Tag::get(ctx.git(), tag)?.name,
            None => {
                let mut tags = Tag::list(ctx.git())?
                    .into_iter()
                    .map(|t| t.name)
                    .collect::<Vec<_>>();
                let idx = ctx.fzf_search("Select a tag to remove", &tags, None)?;
                take(&mut tags[idx])
            }
        };

        ctx.git()
            .execute(["tag", "-d", &tag], format!("Remove local tag {tag:?}"))?;

        ctx.git().execute(
            ["push", "origin", "--delete", &tag],
            format!("Remove remote tag {tag:?}"),
        )?;

        Ok(())
    }

    fn complete_command() -> clap::Command {
        clap::Command::new("tag").arg(complete::tag_arg())
    }
}

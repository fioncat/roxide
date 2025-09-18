use std::mem::take;

use anyhow::Result;
use async_trait::async_trait;
use clap::Args;

use crate::cmd::Command;
use crate::config::context::ConfigContext;
use crate::debug;
use crate::exec::git::tag::Tag;

/// Remove a tag from both local and remote.
#[derive(Debug, Args)]
pub struct RemoveTagCommand {
    /// Specify the tag name to remove. Defaults to removing the latest tag.
    pub tag: Option<String>,
}

#[async_trait]
impl Command for RemoveTagCommand {
    fn name() -> &'static str {
        "tag"
    }

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
}

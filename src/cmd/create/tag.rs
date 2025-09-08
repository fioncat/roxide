use anyhow::Result;
use async_trait::async_trait;
use clap::Args;
use console::style;

use crate::cmd::Command;
use crate::cmd::complete;
use crate::config::context::ConfigContext;
use crate::exec::git::tag::{Tag, UpdateTagRule};
use crate::{confirm, debug, info, output};

#[derive(Debug, Args)]
pub struct CreateTagCommand {
    pub tag: String,
}

#[async_trait]
impl Command for CreateTagCommand {
    async fn run(self, ctx: ConfigContext) -> Result<()> {
        debug!("[cmd] Run create tag command: {:?}", self);

        let tag = match UpdateTagRule::from_str(&self.tag) {
            Some(rule) => {
                let latest = Tag::get_latest(ctx.git())?;
                info!("Latest tag is: {}", style(&latest.name).green());
                let new_tag = rule.apply(&latest.name)?;
                confirm!("Do you want to create tag {}", style(&new_tag).magenta());
                new_tag
            }
            None => {
                let tags = Tag::list(ctx.git())?;
                for t in tags {
                    if t.name == self.tag {
                        output!("Tag {:?} already exists", self.tag);
                        return Ok(());
                    }
                }
                self.tag
            }
        };

        ctx.git()
            .execute(["tag", &tag], format!("Create tag {tag:?}"))?;

        ctx.git().execute(
            ["push", "origin", &tag],
            format!("Push tag {tag:?} to remote"),
        )?;

        Ok(())
    }

    fn complete_command() -> clap::Command {
        clap::Command::new("tag").arg(complete::tag_method_arg())
    }
}

use anyhow::Result;
use async_trait::async_trait;
use clap::Args;
use console::style;

use crate::cmd::complete;
use crate::cmd::{Command, ConfigArgs};
use crate::exec::git;
use crate::exec::git::tag::{Tag, UpdateTagRule};
use crate::{confirm, debug, info, output};

#[derive(Debug, Args)]
pub struct CreateTagCommand {
    pub tag: String,

    #[clap(flatten)]
    pub config: ConfigArgs,
}

#[async_trait]
impl Command for CreateTagCommand {
    async fn run(self) -> Result<()> {
        self.config.build_ctx()?;
        debug!("[cmd] Run create tag command: {:?}", self);

        let tag = match UpdateTagRule::from_str(&self.tag) {
            Some(rule) => {
                let latest = Tag::get_latest(None::<&str>, false)?;
                info!("Latest tag is: {}", style(&latest.name).green());
                let new_tag = rule.apply(&latest.name)?;
                confirm!("Do you want to create tag {}", style(&new_tag).magenta());
                new_tag
            }
            None => {
                let tags = Tag::list(None::<&str>, false)?;
                for t in tags {
                    if t.name == self.tag {
                        output!("Tag {:?} already exists", self.tag);
                        return Ok(());
                    }
                }
                self.tag
            }
        };

        git::new(
            ["tag", &tag],
            None::<&str>,
            format!("Create tag {tag:?}"),
            false,
        )
        .execute()?;

        git::new(
            ["push", "origin", &tag],
            None::<&str>,
            format!("Push tag {tag:?} to remote"),
            true,
        )
        .execute()?;

        Ok(())
    }

    fn complete_command() -> clap::Command {
        clap::Command::new("tag").arg(complete::tag_method_arg())
    }
}

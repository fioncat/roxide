use std::mem::take;

use anyhow::Result;
use async_trait::async_trait;
use clap::Args;

use crate::cmd::complete;
use crate::cmd::{Command, ConfigArgs};
use crate::debug;
use crate::exec::git::tag::Tag;
use crate::exec::{fzf, git};

#[derive(Debug, Args)]
pub struct RemoveTagCommand {
    pub tag: Option<String>,

    #[clap(flatten)]
    pub config: ConfigArgs,
}
#[async_trait]
impl Command for RemoveTagCommand {
    async fn run(self) -> Result<()> {
        self.config.build_ctx()?;
        debug!("[cmd] Run remove tag command: {:?}", self);

        let tag = match self.tag {
            Some(ref tag) => Tag::get(None::<&str>, false, tag)?.name,
            None => {
                let mut tags = Tag::list(None::<&str>, false)?
                    .into_iter()
                    .map(|t| t.name)
                    .collect::<Vec<_>>();
                let idx = fzf::search("Select a tag to remove", &tags, None)?;
                take(&mut tags[idx])
            }
        };

        git::new(
            ["tag", "-d", &tag],
            None::<&str>,
            format!("Remove local tag {tag:?}"),
            false,
        )
        .execute()?;

        git::new(
            ["push", "origin", "--delete", &tag],
            None::<&str>,
            format!("Remove remote tag {tag:?}"),
            false,
        )
        .execute()?;

        Ok(())
    }

    fn complete_command() -> clap::Command {
        clap::Command::new("tag").arg(complete::tag_arg())
    }
}

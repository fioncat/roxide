use anyhow::{Result, bail};
use async_trait::async_trait;
use clap::Args;

use crate::cmd::complete;
use crate::config::context::ConfigContext;
use crate::debug;
use crate::repo::current::get_current_repo;
use crate::repo::mirror::{MirrorSelector, clean_mirrors, remove_mirror};

use super::Command;

/// Remove a mirror for current repository.
#[derive(Debug, Args)]
pub struct RemoveMirrorCommand {
    /// The mirror name to remove, default will remove all mirrors for the current repository.
    pub name: Option<String>,
}

#[async_trait]
impl Command for RemoveMirrorCommand {
    async fn run(self, ctx: ConfigContext) -> Result<()> {
        debug!("[cmd] Run remove mirror command: {:?}", self);
        ctx.lock()?;

        let repo = get_current_repo(&ctx)?;
        let Some(name) = self.name else {
            return clean_mirrors(&ctx, &repo);
        };

        let selector = MirrorSelector::new(&ctx, &repo);
        let mirror = selector.select_one(Some(name), false)?;
        if mirror.new_created {
            bail!("mirror {:?} does not exist", mirror.name);
        }

        remove_mirror(&ctx, &repo, mirror)?;
        Ok(())
    }

    fn complete_command() -> clap::Command {
        clap::Command::new("mirror").args([complete::mirror_name_arg()])
    }
}

use anyhow::{bail, Result};
use clap::Args;

use crate::cmd::Run;
use crate::term::{Cmd, GitTag};
use crate::{config, confirm};

/// Create a release tag use given rule
#[derive(Args)]
pub struct ReleaseArgs {
    /// Rlease rule name
    pub rule: String,
}

impl Run for ReleaseArgs {
    fn run(&self) -> Result<()> {
        let tag = GitTag::latest()?;
        let rule = match config::base().release.get(&self.rule) {
            Some(rule) => rule,
            None => bail!("Could not find rule {}", self.rule),
        };

        let new_tag = tag.apply_rule(rule)?;
        confirm!(
            "Do you want to release: {} -> {}",
            tag.as_str(),
            new_tag.as_str()
        );

        Cmd::git(&["tag", new_tag.as_str()]).execute()?.check()?;
        Cmd::git(&["push", "origin", "tag", new_tag.as_str()])
            .execute()?
            .check()?;

        Ok(())
    }
}

use anyhow::{bail, Result};
use clap::Args;

use crate::cmd::Run;
use crate::shell::{GitTag, Shell};

/// Git tag operations
#[derive(Args)]
pub struct TagArgs {
    /// Tag name
    pub tag: String,

    /// Create a new tag
    #[clap(long, short)]
    pub create: bool,

    /// Delete tag
    #[clap(long, short)]
    pub delete: bool,

    /// Push change (create or delete) to the remote
    #[clap(long, short)]
    pub push: bool,
}

impl Run for TagArgs {
    fn run(&self) -> Result<()> {
        let tags = GitTag::list()?;
        if self.create {
            let mut found = false;
            for tag in tags.iter() {
                if tag.as_str() == self.tag.as_str() {
                    found = true;
                    break;
                }
            }
            if !found {
                Shell::git(&["tag", self.tag.as_str()]).execute()?.check()?;
            }
            if self.push {
                Shell::git(&["push", "origin", "tag", self.tag.as_str()])
                    .execute()?
                    .check()?;
            }
        } else if self.delete {
            let mut found = false;
            for tag in tags.iter() {
                if tag.as_str() == self.tag.as_str() {
                    found = true;
                    break;
                }
            }
            if found {
                Shell::git(&["tag", "-d", self.tag.as_str()])
                    .execute()?
                    .check()?;
            }
            if self.push {
                Shell::git(&["push", "--delete", "origin", self.tag.as_str()])
                    .execute()?
                    .check()?;
            }
        } else {
            for tag in tags.iter() {
                if tag.as_str() == self.tag.as_str() {
                    Shell::git(&["checkout", self.tag.as_str()])
                        .execute()?
                        .check()?;
                    return Ok(());
                }
            }
            bail!("Could not find tag {}", self.tag);
        }
        Ok(())
    }
}

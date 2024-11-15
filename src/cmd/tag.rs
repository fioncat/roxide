use anyhow::{bail, Result};
use clap::Args;

use crate::cmd::{Completion, CompletionResult, Run};
use crate::config::Config;
use crate::confirm;
use crate::exec::Cmd;
use crate::git::GitTag;

/// Git tag operations
#[derive(Args)]
pub struct TagArgs {
    /// Tag name
    pub tag: Option<String>,

    /// Create a new tag
    #[clap(short, long)]
    pub create: bool,

    /// Delete tag
    #[clap(short, long)]
    pub delete: bool,

    /// Push change (create or delete) to the remote
    #[clap(short, long)]
    pub push: bool,

    /// Apply release rule to tag. Enable this will create a new tag and ignore `-c`
    #[clap(short, long)]
    pub rule: Option<String>,
}

impl Run for TagArgs {
    fn run(&self, cfg: &Config) -> Result<()> {
        if let Some(rule) = self.rule.as_ref() {
            let rule = match cfg.release.get(rule) {
                Some(rule) => rule,
                None => bail!("could not find release rule '{rule}'"),
            };
            let tag = match self.tag.as_ref() {
                Some(tag) => GitTag::get(tag),
                None => GitTag::latest(),
            }?;

            let new_tag = tag.apply_rule(rule)?;
            confirm!(
                "Do you want to release: {} -> {}",
                tag.as_str(),
                new_tag.as_str()
            );

            return self.create_tag(new_tag);
        }

        if self.create {
            let tag = match self.tag.as_ref() {
                Some(tag) => GitTag::new(tag),
                None => bail!("please provide tag to create"),
            };
            return self.create_tag(tag);
        }

        if self.delete {
            match self.tag.as_ref() {
                Some(tag) => {
                    if let Ok(tag) = GitTag::get(tag) {
                        Cmd::git(&["tag", "-d", tag.as_str()])
                            .with_display_cmd()
                            .execute()?;
                    }
                }
                None => bail!("please provide tag to delete"),
            };
            if self.push {
                Cmd::git(&[
                    "push",
                    "--delete",
                    "origin",
                    self.tag.as_ref().unwrap().as_str(),
                ])
                .with_display_cmd()
                .execute()?;
            }

            return Ok(());
        }

        match self.tag.as_ref() {
            Some(tag) => Cmd::git(&["checkout", tag]).with_display_cmd().execute()?,
            None => {
                let tags = GitTag::list()?;
                for tag in tags {
                    println!("{tag}");
                }
            }
        };

        Ok(())
    }
}

impl TagArgs {
    fn create_tag(&self, tag: GitTag) -> Result<()> {
        let tags = GitTag::list()?;
        if !tags.iter().any(|t| t.as_str() == tag.as_str()) {
            Cmd::git(&["tag", tag.as_str()])
                .with_display_cmd()
                .execute()?;
        }
        if self.push {
            Cmd::git(&["push", "origin", tag.as_str()])
                .with_display_cmd()
                .execute()?;
        }
        Ok(())
    }

    pub fn completion() -> Completion {
        Completion {
            args: |_cfg, args| match args.len() {
                0 | 1 => {
                    let tags = GitTag::list()?;
                    let items: Vec<_> = tags.into_iter().map(|tag| tag.to_string()).collect();
                    Ok(CompletionResult::from(items))
                }
                _ => Ok(CompletionResult::empty()),
            },
            flags: Some(|cfg, flag, _to_complete| match flag {
                'r' => {
                    let mut rules: Vec<_> = cfg.release.keys().map(|key| key.to_string()).collect();
                    rules.sort();
                    Ok(Some(CompletionResult::from(rules)))
                }
                _ => Ok(None),
            }),
        }
    }
}

use anyhow::Result;
use async_trait::async_trait;
use clap::Args;
use console::style;

use crate::cmd::Command;
use crate::cmd::complete::{CompleteArg, CompleteCommand, funcs};
use crate::config::context::ConfigContext;
use crate::exec::git::tag::{Tag, UpdateTagRule};
use crate::{confirm, debug, info, output};

/// Create a new tag and push it to the remote.
#[derive(Debug, Args)]
pub struct CreateTagCommand {
    /// Two forms: tag name or tag rule. Available rules: [patch, minor, major, date,
    /// date-dash, date-dot].
    /// When using rules, applies the rule to the latest tag to generate a new tag.
    /// First three rules are for semantic versioning, last three for date-based tags.
    /// Examples: patch: v0.4.2 -> v0.4.3, minor: v1.5.6 -> v1.6.0, major: 2.3.4 -> 3.0.0,
    /// date-dash: 2024-01-02, date-dot: 2024.01.02, date: auto-detects dash or dot format
    /// from latest tag.
    /// If not one of these rules, creates the tag directly. For example: "v1.2.3", "1.4.0"
    pub tag: String,
}

#[async_trait]
impl Command for CreateTagCommand {
    fn name() -> &'static str {
        "tag"
    }

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
            .execute(["tag", &tag], format!("Creating tag {tag:?}"))?;

        ctx.git().execute(
            ["push", "origin", &tag],
            format!("Pushing tag {tag:?} to remote"),
        )?;

        Ok(())
    }

    fn complete() -> CompleteCommand {
        Self::default_complete().arg(CompleteArg::new().complete(funcs::complete_tag_method))
    }
}

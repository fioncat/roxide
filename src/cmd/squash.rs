use anyhow::Result;
use async_trait::async_trait;
use clap::Args;

use crate::config::context::ConfigContext;
use crate::debug;
use crate::repo::current::get_current_repo;
use crate::repo::ops::{RepoOperator, SquashOptions};

use super::{CacheArgs, Command, UpstreamArgs, complete};

/// Squash the current branch, combining multiple commits into one.
#[derive(Debug, Args)]
pub struct SquashCommand {
    /// The target branch to squash. If not specified, use the default branch.
    pub target: Option<String>,

    #[clap(flatten)]
    pub cache: CacheArgs,

    #[clap(flatten)]
    pub upstream: UpstreamArgs,

    /// The commit message for the squashed commit. If not specified, git might open
    /// an editor to edit the message.
    #[arg(long, short)]
    pub message: Option<String>,
}

#[async_trait]
impl Command for SquashCommand {
    async fn run(self, ctx: ConfigContext) -> Result<()> {
        debug!("[cmd] Run display command: {:?}", self);
        ctx.lock()?;

        let repo = get_current_repo(&ctx)?;

        let op = RepoOperator::load(&ctx, &repo)?;
        op.squash(SquashOptions {
            target: self.target.as_deref().unwrap_or_default(),
            upstream: self.upstream.enable,
            force_no_cache: self.cache.force_no_cache,
            message: &self.message,
        })
        .await?;

        Ok(())
    }

    fn complete_command() -> clap::Command {
        clap::Command::new("squash").arg(complete::branch_arg())
    }
}

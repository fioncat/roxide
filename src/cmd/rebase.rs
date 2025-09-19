use anyhow::Result;
use async_trait::async_trait;
use clap::Args;

use crate::cmd::complete::{CompleteArg, CompleteCommand, funcs};
use crate::config::context::ConfigContext;
use crate::debug;
use crate::repo::current::get_current_repo;
use crate::repo::ops::{RebaseOptions, RepoOperator};

use super::{CacheArgs, Command, UpstreamArgs};

/// Rebase the current branch.
#[derive(Debug, Args)]
pub struct RebaseCommand {
    /// The target branch to rebase. If not specified, use the default branch.
    pub target: Option<String>,

    #[clap(flatten)]
    pub cache: CacheArgs,

    #[clap(flatten)]
    pub upstream: UpstreamArgs,
}

#[async_trait]
impl Command for RebaseCommand {
    fn name() -> &'static str {
        "rebase"
    }

    async fn run(self, ctx: ConfigContext) -> Result<()> {
        debug!("[cmd] Run display command: {:?}", self);

        let repo = get_current_repo(&ctx)?;

        let op = RepoOperator::load(&ctx, &repo)?;
        op.rebase(RebaseOptions {
            target: self.target.as_deref().unwrap_or_default(),
            upstream: self.upstream.enable,
            force_no_cache: self.cache.force_no_cache,
        })
        .await?;

        Ok(())
    }

    fn complete() -> CompleteCommand {
        Self::default_complete()
            .arg(CompleteArg::new().complete(funcs::complete_branch))
            .arg(CacheArgs::complete())
            .arg(UpstreamArgs::complete())
    }
}

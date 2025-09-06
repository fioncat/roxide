use anyhow::Result;
use async_trait::async_trait;
use clap::Args;

use crate::cmd::complete;
use crate::debug;
use crate::repo::current::get_current_repo;
use crate::repo::ops::{RebaseOptions, RepoOperator};

use super::{Command, ConfigArgs};

#[derive(Debug, Args)]
pub struct RebaseCommand {
    pub target: Option<String>,

    #[arg(long, short)]
    pub force_no_cache: bool,

    #[arg(long, short)]
    pub upstream: bool,

    #[clap(flatten)]
    pub config: ConfigArgs,
}

#[async_trait]
impl Command for RebaseCommand {
    async fn run(self) -> Result<()> {
        debug!("[cmd] Run display command: {:?}", self);
        let ctx = self.config.build_ctx()?;
        ctx.lock()?;

        let repo = get_current_repo(ctx.clone())?;

        let op = RepoOperator::new(ctx.as_ref(), &repo, false)?;
        op.rebase(RebaseOptions {
            target: self.target.as_deref().unwrap_or_default(),
            upstream: self.upstream,
            force_no_cache: self.force_no_cache,
        })
        .await?;

        Ok(())
    }

    fn complete_command() -> clap::Command {
        clap::Command::new("rebase").arg(complete::branch_arg())
    }
}

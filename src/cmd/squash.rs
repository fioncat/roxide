use anyhow::Result;
use async_trait::async_trait;
use clap::Args;

use crate::cmd::complete;
use crate::debug;
use crate::repo::current::get_current_repo;
use crate::repo::ops::{RepoOperator, SquashOptions};

use super::{Command, ConfigArgs};

#[derive(Debug, Args)]
pub struct SquashCommand {
    pub target: Option<String>,

    #[arg(long, short)]
    pub upstream: bool,

    #[arg(long, short)]
    pub force_no_cache: bool,

    #[arg(long, short)]
    pub message: Option<String>,

    #[clap(flatten)]
    pub config: ConfigArgs,
}

#[async_trait]
impl Command for SquashCommand {
    async fn run(self) -> Result<()> {
        let ctx = self.config.build_ctx()?;
        debug!("[cmd] Run display command: {:?}", self);
        ctx.lock()?;

        let repo = get_current_repo(ctx.clone())?;

        let op = RepoOperator::new(ctx.as_ref(), &repo, false)?;
        op.squash(SquashOptions {
            target: self.target.as_deref().unwrap_or_default(),
            upstream: self.upstream,
            force_no_cache: self.force_no_cache,
            message: &self.message,
        })
        .await?;

        Ok(())
    }

    fn complete_command() -> clap::Command {
        clap::Command::new("squash").arg(complete::branch_arg())
    }
}

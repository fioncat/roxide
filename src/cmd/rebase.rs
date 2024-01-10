use anyhow::Result;
use clap::Args;

use crate::cmd::{self, Completion, Run};
use crate::config::Config;
use crate::term::Cmd;

/// Rebase the current branch
#[derive(Args)]
pub struct RebaseArgs {
    /// Rebase source (optional), default will use HEAD branch
    pub target: Option<String>,

    /// Upstream mode, only used for forked repo
    #[clap(short)]
    pub upstream: bool,

    /// When calling the remote API, ignore caches that are not expired.
    #[clap(short)]
    pub force: bool,
}

impl Run for RebaseArgs {
    fn run(&self, cfg: &Config) -> Result<()> {
        let remote = cmd::get_git_remote(cfg, self.upstream, self.force)?;

        let branch = match &self.target {
            Some(target) => Some(target.as_str()),
            None => None,
        };

        let target = remote.target(branch)?;

        Cmd::git(&["rebase", target.as_str()])
            .with_display_cmd()
            .execute()
    }
}

impl RebaseArgs {
    pub fn completion() -> Completion {
        Completion {
            args: Completion::branch_args,
            flags: None,
        }
    }
}

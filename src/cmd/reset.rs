use anyhow::Result;
use clap::Args;

use crate::cmd::{self, Completion, Run};
use crate::config::Config;
use crate::term::Cmd;

/// Reset current branch
#[derive(Args)]
pub struct ResetArgs {
    /// Rebase source (optional), default will use HEAD branch
    pub target: Option<String>,

    /// Upstream mode, only used for forked repo
    #[clap(short)]
    pub upstream: bool,

    /// If true, the cache will not be used when calling the API search.
    #[clap(short)]
    pub force: bool,
}

impl Run for ResetArgs {
    fn run(&self, cfg: &Config) -> Result<()> {
        let remote = cmd::get_git_remote(cfg, self.upstream, self.force)?;
        let branch = match &self.target {
            Some(target) => Some(target.as_str()),
            None => None,
        };

        let target = remote.target(branch)?;

        Cmd::git(&["reset", "--hard", target.as_str()])
            .with_display_cmd()
            .execute_check()
    }
}

impl ResetArgs {
    pub fn completion() -> Completion {
        Completion {
            args: Completion::branch_args,
            flags: None,
        }
    }
}

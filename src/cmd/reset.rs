use anyhow::Result;
use clap::Args;

use crate::cmd::{self, Completion, Run};
use crate::config::Config;
use crate::exec::Cmd;

/// Reset the current branch
#[derive(Args)]
pub struct ResetArgs {
    /// Rebase source (optional), default will use HEAD branch
    pub target: Option<String>,

    /// Upstream mode, only used for forked repo
    #[clap(short, long)]
    pub upstream: bool,

    /// When calling the remote API, ignore caches that are not expired.
    #[clap(short, long)]
    pub force: bool,
}

impl Run for ResetArgs {
    fn run(&self, cfg: &Config) -> Result<()> {
        let remote = cmd::get_git_remote(cfg, self.upstream, self.force)?;
        let branch = self.target.as_deref();

        let target = remote.target(branch)?;

        Cmd::git(&["reset", "--hard", target.as_str()])
            .with_display_cmd()
            .execute()
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

use anyhow::Result;
use clap::Args;

use crate::api;
use crate::cmd::Run;
use crate::config;
use crate::repo::database::Database;
use crate::term::{self, Cmd, GitRemote};

/// Reset current branch
#[derive(Args)]
pub struct ResetArgs {
    /// Rebase source (optional), default will use HEAD branch
    pub target: Option<String>,

    /// Upstream mode, only used for forked repo
    #[clap(long, short)]
    pub upstream: bool,

    /// If true, the cache will not be used when calling the API search.
    #[clap(long, short)]
    pub force: bool,
}

impl Run for ResetArgs {
    fn run(&self) -> Result<()> {
        term::ensure_no_uncommitted()?;
        let remote = if self.upstream {
            let db = Database::read()?;
            let repo = db.must_current()?;
            let remote = config::must_get_remote(repo.remote.as_str())?;
            let provider = api::init_provider(&remote, self.force)?;

            GitRemote::from_upstream(&remote, &repo, &provider)?
        } else {
            GitRemote::new()
        };

        let branch = match &self.target {
            Some(target) => Some(target.as_str()),
            None => None,
        };

        let target = remote.target(branch)?;

        Cmd::git(&["reset", "--hard", target.as_str()])
            .execute()?
            .check()?;

        Ok(())
    }
}

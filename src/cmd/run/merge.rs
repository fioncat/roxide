use anyhow::bail;
use anyhow::Result;
use clap::Args;
use roxide::api;
use roxide::config;
use roxide::repo::database::Database;
use roxide::shell;

use crate::cmd::Run;

/// Create or open PullRequest (MeregeRequest for Gitlab)
#[derive(Args)]
pub struct MergeArgs {
    /// Upstream mode, only used for forked repo
    #[clap(long, short)]
    pub upstream: bool,

    /// Source branch, default will use current branch
    #[clap(long, short)]
    pub source: Option<String>,

    /// Target branch, default will use HEAD branch
    #[clap(long, short)]
    pub target: Option<String>,
}

impl Run for MergeArgs {
    fn run(&self) -> Result<()> {
        shell::ensure_no_uncommitted()?;
        let db = Database::read()?;
        let repo = db.must_current()?;

        let remote = config::must_get_remote(repo.remote.as_str())?;
        let provider = api::init_provider(&remote, false)?;

        let mut api_repo = provider.get_repo(repo.owner.as_str(), repo.name.as_str())?;
        if self.upstream {
            if let None = &api_repo.upstream {
                bail!("The repo {} does not have an upstream", repo.long_name());
            }
        } else {
            if let Some(_) = &api_repo.upstream {
                api_repo.upstream = None;
            }
        }

        todo!()
    }
}

impl MergeArgs {}

use std::path::PathBuf;

use anyhow::Result;
use clap::Args;

use crate::cmd::Run;
use crate::repo::database::Database;
use crate::term::GitBranch;
use crate::{api, config, utils};

/// Open current repository in default browser
#[derive(Args)]
pub struct OpenArgs {
    /// Open current branch
    #[clap(long, short)]
    pub branch: bool,

    /// If true, the cache will not be used when calling the API search.
    #[clap(long, short)]
    pub force: bool,
}

impl Run for OpenArgs {
    fn run(&self) -> Result<()> {
        let db = Database::read()?;
        let repo = db.must_current()?;
        let remote = config::must_get_remote(repo.remote.as_str())?;

        let provider = api::init_provider(&remote, self.force)?;

        let api_repo = provider.get_repo(&repo.owner, &repo.name)?;
        let mut url = api_repo.web_url;

        if self.branch {
            let branch = GitBranch::current()?;
            let path = PathBuf::from(url).join("tree").join(branch);
            url = format!("{}", path.display());
        }

        utils::open_url(&url)
    }
}

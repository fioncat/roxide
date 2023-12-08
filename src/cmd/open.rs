use std::path::PathBuf;

use anyhow::Result;
use clap::Args;

use crate::cmd::Run;
use crate::config::Config;
use crate::repo::database::Database;
use crate::term::GitBranch;
use crate::{api, utils};

/// Open current repository in default browser
#[derive(Args)]
pub struct OpenArgs {
    /// Open current branch
    #[clap(short)]
    pub branch: bool,

    /// If true, the cache will not be used when calling the API search.
    #[clap(short)]
    pub force: bool,
}

impl Run for OpenArgs {
    fn run(&self, cfg: &Config) -> Result<()> {
        let db = Database::load(cfg)?;
        let repo = db.must_get_current()?;

        let provider = api::build_provider(cfg, &repo.remote, self.force)?;

        let api_repo = provider.get_repo(&repo.owner.name, &repo.name)?;
        let mut url = api_repo.web_url;

        if self.branch {
            let branch = GitBranch::current()?;
            let path = PathBuf::from(url).join("tree").join(branch);
            url = format!("{}", path.display());
        }

        utils::open_url(&url)
    }
}

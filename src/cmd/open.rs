use std::path::PathBuf;

use anyhow::{bail, Result};
use clap::Args;

use crate::api::{ActionOptions, ActionTarget, Provider};
use crate::cmd::Run;
use crate::config::Config;
use crate::repo::database::Database;
use crate::repo::Repo;
use crate::term::{Cmd, GitBranch};
use crate::{api, utils};

/// Open current repository in default browser
#[derive(Args)]
pub struct OpenArgs {
    /// Open current branch
    #[clap(short)]
    pub branch: bool,

    /// Open workflow action run (pipline in gitlab) for currect commit or branch.
    #[clap(short)]
    pub action: bool,

    /// When calling the remote API, ignore caches that are not expired.
    #[clap(short)]
    pub force: bool,
}

impl Run for OpenArgs {
    fn run(&self, cfg: &Config) -> Result<()> {
        let db = Database::load(cfg)?;
        let repo = db.must_get_current()?;

        let provider = api::build_provider(cfg, &repo.remote_cfg, self.force)?;

        if self.action {
            return self.open_action(repo, provider.as_ref());
        }

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

impl OpenArgs {
    fn open_action(&self, repo: Repo, provider: &dyn Provider) -> Result<()> {
        let target = if self.branch {
            let branch = GitBranch::current()?;
            ActionTarget::Branch(branch)
        } else {
            let sha = Cmd::git(&["rev-parse", "HEAD"])
                .with_display("Get current commit")
                .read()?;
            ActionTarget::Commit(sha)
        };

        let opts = ActionOptions {
            owner: repo.owner.into_owned(),
            name: repo.name.into_owned(),
            target,
        };

        let url = provider.get_action(opts)?;
        if url.is_none() {
            let desc = if self.branch { "branch" } else { "commit" };
            bail!("cannot find action run for current {desc}");
        }

        utils::open_url(url.unwrap())
    }
}

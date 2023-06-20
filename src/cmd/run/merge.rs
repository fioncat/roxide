use anyhow::bail;
use anyhow::Result;
use clap::Args;
use roxide::api;
use roxide::api::types::MergeOptions;
use roxide::config;
use roxide::confirm;
use roxide::info;
use roxide::repo::database::Database;
use roxide::shell;
use roxide::shell::GitBranch;
use roxide::utils;

use crate::cmd::Run;

/// Create or open PullRequest (MeregeRequest for Gitlab)
#[derive(Args)]
pub struct MergeArgs {
    /// Target branch, default will use HEAD branch
    pub target: Option<String>,

    /// Upstream mode, only used for forked repo
    #[clap(long, short)]
    pub upstream: bool,

    /// If true, the cache will not be used when calling the API search.
    #[clap(long, short)]
    pub force: bool,
}

impl Run for MergeArgs {
    fn run(&self) -> Result<()> {
        shell::ensure_no_uncommitted()?;
        let db = Database::read()?;
        let repo = db.must_current()?;

        let remote = config::must_get_remote(repo.remote.as_str())?;
        let provider = api::init_provider(&remote, self.force)?;

        info!("Get repo info from remote API");
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

        let target = match &self.target {
            Some(t) => t.clone(),
            None => match &api_repo.upstream {
                Some(upstream) => {
                    if upstream.default_branch.is_empty() {
                        bail!("Upstream default branch is empty in api info");
                    }
                    upstream.default_branch.clone()
                }
                None => GitBranch::default()?,
            },
        };

        let source = GitBranch::current()?;

        if !self.upstream && target == source {
            bail!("Could not merge myself");
        }

        let merge = MergeOptions {
            owner: format!("{}", repo.owner),
            name: api_repo.name,
            upstream: api_repo.upstream,
            source,
            target,
        };

        info!("Get merge info from remote API");
        if let Some(url) = provider.get_merge(merge.clone())? {
            utils::open_url(&url)?;
            return Ok(());
        }

        println!();
        println!("About to create merge: {}", merge.pretty_display());
        confirm!("Continue");
        let title = utils::input("Please input title", true, None)?;
        let body = if utils::confirm("Do you need body")? {
            utils::edit("", ".md", true)?
        } else {
            String::new()
        };

        info!("Call remote API to create merge");
        let url = provider.create_merge(merge, title, body)?;
        utils::open_url(&url)?;

        Ok(())
    }
}

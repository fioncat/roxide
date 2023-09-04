use anyhow::bail;
use anyhow::Result;
use clap::Args;

use crate::api;
use crate::api::types::MergeOptions;
use crate::cmd::Run;
use crate::config;
use crate::repo::database::Database;
use crate::shell::{self, GitBranch, GitRemote};
use crate::utils;
use crate::{confirm, info};

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
        let mut provider = api::init_provider(&remote, self.force)?;

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
            name: format!("{}", repo.name),
            upstream: api_repo.upstream,
            source,
            target,
        };

        info!("Get merge info from remote API");
        if let Some(url) = provider.get_merge(merge.clone())? {
            utils::open_url(&url)?;
            return Ok(());
        }

        let git_remote = if self.upstream {
            GitRemote::from_upstream(&remote, &repo, &provider)?
        } else {
            GitRemote::new()
        };
        let commits = git_remote.commits_between(Some(&merge.target))?;
        if commits.is_empty() {
            bail!("No commit to merge");
        }

        let (commit_desc, init_title) = if commits.len() == 1 {
            (String::from("1 commit"), Some(commits[0].as_str()))
        } else {
            (format!("{} commits", commits.len()), None)
        };

        println!();
        println!("About to create merge: {}", merge.pretty_display());
        println!("With {commit_desc}");
        confirm!("Continue");
        let title = shell::input("Please input title", true, init_title)?;
        let body = if shell::confirm("Do you need body")? {
            shell::edit_content("Please input your body (markdown)", "body.md", true)?
        } else {
            String::new()
        };

        info!("Call remote API to create merge");
        let url = provider.create_merge(merge, title, body)?;
        utils::open_url(&url)?;

        Ok(())
    }
}

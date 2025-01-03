use anyhow::{bail, Result};
use clap::Args;

use crate::api::MergeOptions;
use crate::cmd::{Completion, Run};
use crate::config::Config;
use crate::git::{self, GitBranch, GitRemote};
use crate::repo::database::Database;
use crate::term;
use crate::{api, confirm, info, utils};

/// Create or open MergeRequest (PullRequest for Github)
#[derive(Args)]
pub struct MergeArgs {
    /// Target branch, default will use HEAD branch
    pub target: Option<String>,

    /// Upstream mode, only used for forked repo
    #[clap(short, long)]
    pub upstream: bool,

    /// When calling the remote API, ignore caches that are not expired.
    #[clap(short, long)]
    pub force: bool,
}

impl Run for MergeArgs {
    fn run(&self, cfg: &Config) -> Result<()> {
        git::ensure_no_uncommitted()?;
        let db = Database::load(cfg)?;
        let repo = db.must_get_current()?;

        let mut provider = api::build_provider(cfg, &repo.remote_cfg, self.force)?;

        info!("Get repo info from remote API");
        let mut api_repo = provider.get_repo(repo.owner.as_ref(), repo.name.as_ref())?;
        if self.upstream {
            if api_repo.upstream.is_none() {
                bail!(
                    "the repo '{}' does not have an upstream",
                    repo.name_with_remote()
                );
            }
        } else if api_repo.upstream.is_some() {
            api_repo.upstream = None;
        }

        let target = match &self.target {
            Some(t) => t.clone(),
            None => match &api_repo.upstream {
                Some(upstream) => {
                    if upstream.default_branch.is_empty() {
                        bail!("upstream default branch is empty in api info");
                    }
                    upstream.default_branch.clone()
                }
                None => GitBranch::default()?,
            },
        };

        let source = GitBranch::current(false)?;

        if !self.upstream && target == source {
            bail!("could not merge myself");
        }

        let merge = MergeOptions {
            owner: repo.owner.to_string(),
            name: repo.name.to_string(),
            upstream: api_repo.upstream,
            source,
            target,
        };

        info!("Get merge info from remote API");
        if let Some(url) = provider.get_merge(merge.clone())? {
            utils::open_url(url)?;
            return Ok(());
        }

        let git_remote = if self.upstream {
            GitRemote::from_upstream(cfg, &repo, provider.as_ref())?
        } else {
            GitRemote::new()
        };
        let commits = git_remote.commits_between(Some(&merge.target))?;
        if commits.is_empty() {
            bail!("no commit to merge");
        }

        let (commit_desc, init_title) = if commits.len() == 1 {
            (String::from("1 commit"), Some(commits[0].as_str()))
        } else {
            (format!("{} commits", commits.len()), None)
        };

        eprintln!();
        eprintln!("About to create merge: {}", merge.pretty_display());
        eprintln!("With {}", commit_desc);
        confirm!("Continue");

        let title = term::input("Please input title", true, init_title)?;
        let body = if term::confirm("Do you need body")? {
            term::edit_content(cfg, "Please input your body (markdown)", "body.md", true)?
        } else {
            String::new()
        };

        info!("Call remote API to create merge");
        let url = provider.create_merge(merge, title, body)?;

        utils::open_url(url)
    }
}

impl MergeArgs {
    pub fn completion() -> Completion {
        Completion {
            args: Completion::branch_args,
            flags: None,
        }
    }
}

use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::Path;

use anyhow::bail;
use anyhow::{Context, Result};
use async_trait::async_trait;
use clap::Args;
use console::style;

use crate::api::{PullRequest, PullRequestHead};
use crate::cmd::complete::{CompleteArg, CompleteCommand, funcs};
use crate::cmd::{CacheArgs, Command, UpstreamArgs};
use crate::config::context::ConfigContext;
use crate::exec::git::commit::ensure_no_uncommitted_changes;
use crate::repo::current::get_current_repo;
use crate::repo::ops::RepoOperator;
use crate::repo::select::SelectPullRequestsArgs;
use crate::repo::wait_action::WaitActionArgs;
use crate::{confirm, debug, info};

/// Create a new pull request on the remote (alias `pr`)
#[derive(Debug, Args)]
pub struct CreatePullRequestCommand {
    /// The base branch for the pull request. If not specified, use the default branch.
    pub base: Option<String>,

    #[clap(flatten)]
    pub cache: CacheArgs,

    #[clap(flatten)]
    pub upstream: UpstreamArgs,

    #[clap(flatten)]
    pub wait: WaitActionArgs,
}

#[async_trait]
impl Command for CreatePullRequestCommand {
    fn name() -> &'static str {
        "pull-request"
    }

    fn alias() -> Vec<&'static str> {
        vec!["pr"]
    }

    async fn run(self, ctx: ConfigContext) -> Result<()> {
        debug!("[cmd] Running create pull request command: {:?}", self);
        ensure_no_uncommitted_changes(ctx.git())?;

        let repo = get_current_repo(&ctx)?;
        let api = ctx.get_api(&repo.remote, self.cache.force_no_cache)?;
        let args = SelectPullRequestsArgs {
            upstream: self.upstream,
            base: self.base,
            id: None,
            all: false,
        };
        let opts = args
            .build_list_options(&ctx, &repo, api.as_ref(), true)
            .await?;
        debug!("[cmd] List pull request options: {opts:?}");
        if let PullRequestHead::Branch(head_branch) = opts.head.as_ref().unwrap()
            && head_branch == opts.base.as_ref().unwrap()
        {
            bail!("cannot create a pull request with the same head and base");
        }

        let prs = api.list_pull_requests(opts.clone()).await?;
        if !prs.is_empty() {
            return open::that(&prs[0].web_url).with_context(|| {
                format!(
                    "failed to open existing pull request web url {:?}",
                    prs[0].web_url
                )
            });
        }

        let base = opts.base.unwrap();
        let head = opts.head.unwrap();
        info!(
            "About to create a new pull request to {}: {} -> {}",
            style(format!("{}/{}", opts.owner, opts.name)).cyan(),
            style(format!("{head}")).magenta(),
            style(&base).magenta(),
        );

        let op = RepoOperator::load(&ctx, &repo)?;
        let git_remote = op
            .get_git_remote(self.upstream.enable, self.cache.force_no_cache)
            .await?;
        info!("Base git remote is {}", style(git_remote.as_str()).cyan());
        debug!("[cmd] Git remote: {git_remote:?}");
        let mut commits = git_remote.commits_between(ctx.git(), &base, false)?;
        if commits.is_empty() {
            bail!("no new commits to create a pull request");
        }
        info!(
            "Total {} commit(s) to be merged",
            style(commits.len()).magenta()
        );
        confirm!("Continue");

        let edit_file = Path::new(&ctx.cfg.data_dir).join("create_pr.md");
        let init_title = if commits.len() == 1 {
            commits.remove(0)
        } else {
            format!("{head}")
        };
        let mut content = format!("# {init_title}\n\n");

        if commits.len() > 1 {
            for commit in commits {
                content.push_str("* ");
                content.push_str(&commit);
                content.push_str("\n\n");
            }
        }

        fs::write(&edit_file, content).context("write content to edit file")?;
        ctx.edit(&edit_file)?;

        let file = File::open(&edit_file).context("open edit file")?;
        let reader = BufReader::new(file);
        let mut title = None;
        let mut body = String::new();
        for line in reader.lines() {
            let line = line.context("read line from edit file")?;
            if title.is_none() {
                let line = line.trim_start_matches('#').trim();
                title = Some(line.to_string());
                continue;
            }
            body.push_str(line.trim());
            body.push('\n');
        }

        let title = title.unwrap_or_default();
        let title = title.trim();
        if title.is_empty() {
            bail!("pull request title cannot be empty");
        }
        debug!("[cmd] Pull request title: {title:?}");

        let body = body.trim();
        let body = if body.is_empty() {
            None
        } else {
            Some(body.to_string())
        };
        debug!("[cmd] Pull request body: {body:?}");

        let pr = PullRequest {
            base,
            head,
            title: title.to_string(),
            body,
            id: 0,
            web_url: String::new(),
        };
        debug!("[cmd] Creating pull request: {pr:?}");
        let web_url = api
            .create_pull_request(&opts.owner, &opts.name, &pr)
            .await?;

        if self.wait.enable {
            self.wait.wait(&ctx, &repo, api.as_ref()).await?;
        }

        open::that(&web_url).with_context(|| {
            format!(
                "failed to open newly created pull request web url {:?}",
                web_url
            )
        })
    }

    fn complete() -> CompleteCommand {
        Self::default_complete()
            .arg(CompleteArg::new().complete(funcs::complete_branch))
            .arg(CacheArgs::complete())
            .arg(UpstreamArgs::complete())
            .arg(WaitActionArgs::complete())
    }
}

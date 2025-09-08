use anyhow::Result;
use anyhow::bail;
use async_trait::async_trait;
use clap::Args;

use crate::api::HeadRepository;
use crate::api::PullRequestHead;
use crate::cmd::Command;
use crate::config::context::ConfigContext;
use crate::debug;
use crate::exec::git::branch::Branch;
use crate::outputln;
use crate::repo::current::get_current_repo;
use crate::term::list::TableArgs;

#[derive(Debug, Args)]
pub struct ListPullRequestCommand {
    #[arg(long, short)]
    pub upstream: bool,

    #[arg(long, short)]
    pub all: bool,

    #[arg(long, short)]
    pub force_no_cache: bool,

    #[clap(flatten)]
    pub table: TableArgs,
}

#[async_trait]
impl Command for ListPullRequestCommand {
    async fn run(self, ctx: ConfigContext) -> Result<()> {
        debug!("[cmd] Run list pull request command: {:?}", self);

        let repo = get_current_repo(&ctx)?;
        let api = ctx.get_api(&repo.remote, false)?;

        let (owner, name) = if self.upstream {
            let api_repo = api.get_repo(&repo.remote, &repo.owner, &repo.name).await?;
            let Some(upstream) = api_repo.upstream else {
                bail!("repo has no upstream");
            };
            (upstream.owner, upstream.name)
        } else {
            (repo.owner.clone(), repo.name.clone())
        };

        let head = if self.all {
            None
        } else {
            let current_branch = Branch::current(ctx.git())?;
            if self.upstream {
                Some(PullRequestHead::Repository(HeadRepository {
                    owner: repo.owner,
                    name: repo.name,
                    branch: current_branch,
                }))
            } else {
                Some(PullRequestHead::Branch(current_branch))
            }
        };

        let prs = api.list_pull_requests(&owner, &name, head).await?;
        debug!("[cmd] Pull requests: {prs:?}");

        let text = self
            .table
            .render(vec!["ID", "Base", "Head", "Title"], &prs)?;
        outputln!("{text}");
        Ok(())
    }

    fn complete_command() -> clap::Command {
        Self::augment_args(clap::Command::new("pull-request").alias("pr"))
    }
}

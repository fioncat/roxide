use std::path::Path;

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use clap::Args;

use crate::cmd::{Command, complete};
use crate::config::context::ConfigContext;
use crate::exec::git::branch::Branch;
use crate::repo::current::get_current_repo;
use crate::{debug, info};

#[derive(Debug, Args)]
pub struct OpenRepoCommand {
    #[arg(long, short)]
    pub branch: Option<Option<String>>,

    #[arg(long, short)]
    pub force_no_cache: bool,

    #[arg(long, short)]
    pub upstream: bool,
}

#[async_trait]
impl Command for OpenRepoCommand {
    async fn run(self, ctx: ConfigContext) -> Result<()> {
        debug!("[cmd] Run open repo command: {:?}", self);
        ctx.lock()?;

        let repo = get_current_repo(&ctx)?;
        let api = ctx.get_api(&repo.remote, self.force_no_cache)?;

        let mut api_repo = api.get_repo(&repo.remote, &repo.owner, &repo.name).await?;
        if self.upstream {
            let Some(upstream) = api_repo.upstream else {
                bail!("repository {} does not have an upstream", repo.full_name());
            };
            api_repo = api
                .get_repo(&repo.remote, &upstream.owner, &upstream.name)
                .await?;
        }

        let mut web_url = api_repo.web_url;
        if let Some(branch) = self.branch {
            let branch = match branch {
                Some(b) => b,
                None => Branch::current(ctx.git())?,
            };

            let path = Path::new(&web_url).join("tree").join(branch);
            web_url = format!("{}", path.display());
        }

        info!("Open: {web_url:?}");
        open::that(&web_url).with_context(|| format!("cannot open repo web url {web_url:?}"))?;
        Ok(())
    }

    fn complete_command() -> clap::Command {
        clap::Command::new("repo").args([complete::branch_arg().short('b')])
    }
}

use std::rc::Rc;

use anyhow::{bail, Context, Result};
use clap::Args;

use crate::cmd::Run;
use crate::repo::database::Database;
use crate::repo::types::Repo;
use crate::{config, utils};

/// Batch remove repo(s)
#[derive(Args)]
pub struct ClearArgs {
    /// Clear under remote
    pub remote: Option<String>,

    /// Clear under owner
    pub owner: Option<String>,

    /// Remove everything
    #[clap(long)]
    pub all: bool,

    /// Remove repos whose last access interval is greater than or equal to
    /// this value.
    #[clap(long, short)]
    pub duration: Option<String>,

    /// Delete repos whose access times are less than this value.
    #[clap(long, short)]
    pub access: Option<u64>,
}

impl Run for ClearArgs {
    fn run(&self) -> Result<()> {
        let mut db = Database::read()?;
        let repos = match &self.remote {
            Some(remote) => match &self.owner {
                Some(owner) => db.list_by_owner(remote, owner),
                None => db.list_by_remote(remote),
            },
            None => db.list_all(),
        };
        let repos = self.filter(repos)?;
        let items: Vec<_> = repos.iter().map(|repo| repo.full_name()).collect();
        utils::confirm_items(&items, "remove", "removal", "Repo", "Repos")?;

        for repo in repos.into_iter() {
            let path = repo.get_path();
            utils::remove_dir_recursively(path)?;
            db.remove(repo);
        }

        db.close()
    }
}

impl ClearArgs {
    fn filter(&self, repos: Vec<Rc<Repo>>) -> Result<Vec<Rc<Repo>>> {
        if self.all {
            return Ok(repos);
        }
        if let Some(d) = &self.duration {
            let d = utils::parse_duration_secs(d).context("Parse duration")?;
            let now = config::now_secs();

            return Ok(repos
                .into_iter()
                .filter(|repo| {
                    let delta = now.saturating_sub(repo.last_accessed);
                    delta >= d
                })
                .collect());
        }

        if let Some(access) = &self.access {
            let access = *access as f64;
            return Ok(repos
                .into_iter()
                .filter(|repo| repo.accessed <= access)
                .collect());
        }

        bail!("You should provide at least one filter condition without `all`")
    }
}

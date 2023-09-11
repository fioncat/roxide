use std::rc::Rc;

use anyhow::{Context, Result};
use clap::Args;

use crate::cmd::Run;
use crate::repo::database::Database;
use crate::repo::query::{Query, SelectOptions};
use crate::repo::tmp_mark::TmpMark;
use crate::repo::types::Repo;
use crate::{config, confirm, info, term, utils};

/// Remove a repo from database and disk.
#[derive(Args)]
pub struct RemoveArgs {
    /// The remote name.
    pub remote: Option<String>,

    /// The repo query, format is `owner[/[name]]`.
    pub query: Option<String>,

    /// Recursively delete multiple repos.
    #[clap(long, short)]
    pub recursive: bool,

    /// Remove repos whose last access interval is greater than or equal to
    /// this value.
    #[clap(long, short)]
    pub duration: Option<String>,

    /// Remove repos whose access times are less than this value.
    #[clap(long, short)]
    pub access: Option<u64>,

    /// Filter items.
    #[clap(long)]
    pub filter: bool,

    /// Remove tmp repo.
    #[clap(long, short)]
    pub tmp: bool,
}

impl Run for RemoveArgs {
    fn run(&self) -> Result<()> {
        let mut db = Database::read()?;
        if self.recursive || self.tmp {
            self.remove_many(&mut db)?;
        } else {
            self.remove_one(&mut db)?;
        }

        db.close()
    }
}

impl RemoveArgs {
    fn remove_one(&self, db: &mut Database) -> Result<()> {
        let query = Query::from_args(&db, &self.remote, &self.query);
        let (_, repo) =
            query.must_select(SelectOptions::new().with_local_only(true).with_search(true))?;
        confirm!("Do you want to remove repo {}", repo.long_name());

        let path = repo.get_path();
        utils::remove_dir_recursively(path)?;

        db.remove(repo);

        Ok(())
    }

    fn remove_many(&self, db: &mut Database) -> Result<()> {
        let (repos, level, tmp_mark) = if self.tmp {
            let mut tmp_mark = TmpMark::read()?;
            let (repos, level) = tmp_mark.query_remove(&db, &self.remote, &self.query)?;
            (repos, level, Some(tmp_mark))
        } else {
            let query = Query::from_args(&db, &self.remote, &self.query);
            let (repos, level) = query.list_local(self.filter)?;
            let repos = self.filter_many(repos)?;
            (repos, level, None)
        };

        if repos.is_empty() {
            info!("Nothing to remove");
            return Ok(());
        }

        let items: Vec<_> = repos.iter().map(|repo| repo.as_string(&level)).collect();
        term::must_confirm_items(&items, "remove", "removal", "Repo", "Repos")?;

        for repo in repos.into_iter() {
            let path = repo.get_path();
            utils::remove_dir_recursively(path)?;
            db.remove(repo);
        }

        if let Some(tmp_mark) = tmp_mark {
            tmp_mark.save()?;
        }

        Ok(())
    }

    fn filter_many(&self, repos: Vec<Rc<Repo>>) -> Result<Vec<Rc<Repo>>> {
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

        Ok(repos)
    }
}

use anyhow::Result;
use clap::Args;

use crate::cmd::Run;
use crate::repo::database::Database;
use crate::repo::query::Query;
use crate::{confirm, utils};

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
}

impl Run for RemoveArgs {
    fn run(&self) -> Result<()> {
        let mut db = Database::read()?;
        if self.recursive {
        } else {
            self.remove_one(&mut db)?;
        }

        db.close()
    }
}

impl RemoveArgs {
    fn remove_one(&self, db: &mut Database) -> Result<()> {
        let query = Query::from_args(&db, &self.remote, &self.query)
            .with_local_only(true)
            .with_search(true);

        let (_, repo) = query.must_one()?;
        confirm!("Do you want to remove repo {}", repo.long_name());

        let path = repo.get_path();
        utils::remove_dir_recursively(path)?;

        db.remove(repo);

        Ok(())
    }
}

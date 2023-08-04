use std::rc::Rc;

use anyhow::Result;
use clap::Args;

use crate::cmd::Run;
use crate::config::types::Remote;
use crate::repo::database::Database;
use crate::repo::types::Repo;
use crate::{config, confirm, shell, utils};

/// Remove a repo from database and disk.
#[derive(Args)]
pub struct RemoveArgs {
    /// The remote name.
    pub remote: String,

    /// The repo query, format is `owner[/[name]]`.
    pub query: String,
}

impl Run for RemoveArgs {
    fn run(&self) -> Result<()> {
        let mut db = Database::read()?;
        let remote = config::must_get_remote(&self.remote)?;

        let repo = self.get_repo(&remote, &db)?;
        confirm!("Do you want to remove repo {}", repo.long_name());

        let path = repo.get_path();
        utils::remove_dir_recursively(path)?;

        db.remove(repo);

        db.close()
    }
}

impl RemoveArgs {
    fn get_repo(&self, remote: &Remote, db: &Database) -> Result<Rc<Repo>> {
        if self.query.ends_with("/") {
            let mut owner = self.query.trim_end_matches("/");
            if let Some(raw_owner) = remote.owner_alias.get(owner) {
                owner = raw_owner.as_str();
            }
            let mut repos = db.list_by_owner(remote.name.as_str(), owner);
            let items: Vec<_> = repos.iter().map(|repo| format!("{}", repo.name)).collect();

            let idx = shell::search(&items)?;

            return Ok(repos.remove(idx));
        }
        let (owner, name) = utils::parse_query(remote, &self.query);
        db.must_get(&remote.name, &owner, &name)
    }
}

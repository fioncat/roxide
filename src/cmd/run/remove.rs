use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::rc::Rc;

use anyhow::{Context, Result};
use clap::Args;

use crate::cmd::Run;
use crate::config::types::Remote;
use crate::repo::database::Database;
use crate::repo::types::Repo;
use crate::{config, confirm, info, shell, utils};

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
        self.remove_path(path)?;

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

    fn remove_path(&self, path: PathBuf) -> Result<()> {
        info!("Remove dir {}", path.display());
        fs::remove_dir_all(&path).context("Remove directory")?;

        let dir = path.parent();
        if let None = dir {
            return Ok(());
        }
        let mut dir = dir.unwrap();
        loop {
            match fs::read_dir(dir) {
                Ok(dir_read) => {
                    let count = dir_read.count();
                    if count > 0 {
                        return Ok(());
                    }
                    info!("Remove dir {}", dir.display());
                    fs::remove_dir(dir).context("Remove directory")?;
                    match dir.parent() {
                        Some(parent) => dir = parent,
                        None => return Ok(()),
                    }
                }
                Err(err) if err.kind() == ErrorKind::NotFound => return Ok(()),
                Err(err) => return Err(err).with_context(|| format!("Read dir {}", dir.display())),
            }
        }
    }
}

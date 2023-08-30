use std::fs;
use std::io::ErrorKind;
use std::rc::Rc;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Args;

use crate::batch::{self, Reporter, Task};
use crate::cmd::Run;
use crate::config::types::Remote;
use crate::repo::database::Database;
use crate::repo::query::Query;
use crate::repo::types::Repo;
use crate::shell::{GitTask, Shell};
use crate::{info, utils};

/// Batch import repos
#[derive(Args)]
pub struct ImportArgs {
    /// The remote to import.
    pub remote: String,

    /// The owner to import.
    pub owner: String,

    /// If true, the cache will not be used when calling the API search.
    #[clap(long, short)]
    pub force: bool,

    /// If true, filter import repos.
    #[clap(long)]
    pub filter: bool,
}

struct ImportTask {
    remote: Arc<Remote>,
    owner: Arc<String>,

    name: Arc<String>,
}

impl Task<Arc<String>> for ImportTask {
    fn run(&self, rp: &Reporter<Arc<String>>) -> Result<Arc<String>> {
        rp.message(format!("Cloning {}...", self.name));

        let path = Repo::get_workspace_path(
            self.remote.name.as_str(),
            self.owner.as_str(),
            self.name.as_str(),
        );

        match fs::read_dir(&path) {
            Ok(_) => {
                return Ok(Arc::clone(&self.name));
            }
            Err(err) if err.kind() == ErrorKind::NotFound => {}
            Err(err) => return Err(err).with_context(|| format!("Read dir {}", path.display())),
        }

        let url = Repo::get_clone_url(self.owner.as_str(), self.name.as_str(), &self.remote);
        let path = format!("{}", path.display());

        Shell::exec_git_mute(&["clone", url.as_str(), path.as_str()])?;

        let git = GitTask::new(path.as_str());
        if let Some(user) = &self.remote.user {
            git.exec(&["config", "user.name", user.as_str()])?;
        }
        if let Some(email) = &self.remote.email {
            git.exec(&["config", "user.email", email.as_str()])?;
        }
        Ok(Arc::clone(&self.name))
    }

    fn message_done(&self, result: &Result<Arc<String>>) -> String {
        match result {
            Ok(_) => format!("Clone {} done", self.name),
            Err(err) => format!("Clone {} error: {}", self.name, err),
        }
    }
}

impl Run for ImportArgs {
    fn run(&self) -> Result<()> {
        let mut db = Database::read()?;

        let query = Query::new(&db, vec![self.remote.clone(), self.owner.clone()]);
        let (remote, owner, names) = query.list_remote(self.force, self.filter)?;

        if names.is_empty() {
            info!("Nothing to import");
            return Ok(());
        }

        let remote_arc = Arc::new(remote);
        let owner_arc = Arc::new(owner.clone());

        let mut tasks = Vec::with_capacity(names.len());
        for name in names {
            tasks.push(ImportTask {
                remote: remote_arc.clone(),
                owner: owner_arc.clone(),
                name: Arc::new(name),
            })
        }
        drop(remote_arc);
        drop(owner_arc);

        if tasks.is_empty() {
            info!("Nothing to import");
            return Ok(());
        }
        let items: Vec<_> = tasks.iter().map(|task| format!("{}", task.name)).collect();
        utils::confirm_items(&items, "import", "import", "Repo", "Repos")?;

        let remote_rc = Rc::new(self.remote.clone());
        let owner_rc = Rc::new(owner);

        let results = batch::run("Import", tasks);
        for result in results {
            if let Ok(name) = result {
                let name = Arc::try_unwrap(name).unwrap();
                let repo = Repo {
                    remote: remote_rc.clone(),
                    owner: owner_rc.clone(),
                    name: Rc::new(name),
                    path: None,
                    last_accessed: 0,
                    accessed: 0.0,
                };
                db.update(Rc::new(repo));
            }
        }
        drop(remote_rc);
        drop(owner_rc);

        db.close()
    }
}

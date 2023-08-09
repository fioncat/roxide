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
use crate::repo::types::Repo;
use crate::shell::Shell;
use crate::{api, config, utils};

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
    #[clap(long, short)]
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

        if let Some(user) = &self.remote.user {
            Shell::exec_git_mute(&["-C", path.as_str(), "config", "user.name", user.as_str()])?;
        }
        if let Some(email) = &self.remote.email {
            Shell::exec_git_mute(&["-C", path.as_str(), "config", "user.email", email.as_str()])?;
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
        let remote = config::must_get_remote(&self.remote)?;

        let provider = api::init_provider(&remote, self.force)?;
        let mut names: Vec<String> = provider
            .list_repos(&self.owner)?
            .into_iter()
            .filter(|name| {
                if let None = db.get(&self.remote, &self.owner, &name) {
                    true
                } else {
                    false
                }
            })
            .collect();
        if names.is_empty() {
            println!("Nothing to import");
            return Ok(());
        }

        if self.filter {
            names = utils::edit_items(names)?;
        }

        let remote_arc = Arc::new(remote);
        let owner_arc = Arc::new(self.owner.clone());

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

        let items: Vec<_> = tasks.iter().map(|task| format!("{}", task.name)).collect();
        utils::confirm_items(&items, "import", "import", "Repo", "Repos")?;

        let remote_rc = Rc::new(self.remote.clone());
        let owner_rc = Rc::new(self.owner.clone());

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

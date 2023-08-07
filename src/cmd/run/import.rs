use std::fs;
use std::io::ErrorKind;
use std::rc::Rc;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Args;
use console::style;

use crate::batch::{self, Task};
use crate::cmd::Run;
use crate::config::types::Remote;
use crate::repo::database::Database;
use crate::repo::types::Repo;
use crate::shell::Shell;
use crate::{api, config, confirm};

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
}

struct ImportTask {
    remote: Arc<Remote>,
    owner: Arc<String>,

    name: Arc<String>,
}

impl Task<Arc<String>> for ImportTask {
    fn run(&self) -> Result<Arc<String>> {
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

        Shell::git(&["clone", url.as_str(), path.as_str()])
            .piped_stderr()
            .set_mute(true)
            .execute()?
            .check()?;

        if let Some(user) = &self.remote.user {
            Shell::git(&["-C", path.as_str(), "config", "user.name", user.as_str()])
                .piped_stderr()
                .set_mute(true)
                .execute()?
                .check()?;
        }
        if let Some(email) = &self.remote.email {
            Shell::git(&["-C", path.as_str(), "config", "user.email", email.as_str()])
                .set_mute(true)
                .execute()?
                .check()?;
        }
        Ok(Arc::clone(&self.name))
    }

    fn message_running(&self) -> String {
        format!("Cloning {}...", self.name)
    }

    fn message_done(&self, result: &Result<Arc<String>>) -> String {
        match result {
            Ok(_) => format!("Clone {} done", self.name),
            Err(_) => {
                let msg = format!("Clone {} failed", self.name);
                format!("{}", style(msg).red())
            }
        }
    }
}

impl Run for ImportArgs {
    fn run(&self) -> Result<()> {
        let mut db = Database::read()?;
        let remote = config::must_get_remote(&self.remote)?;

        let provider = api::init_provider(&remote, self.force)?;
        let names = provider.list_repos(&self.owner)?;

        let remote_arc = Arc::new(remote);
        let owner_arc = Arc::new(self.owner.clone());

        let mut tasks = Vec::with_capacity(names.len());
        for name in names {
            if let None = db.get(&self.remote, &self.owner, &name) {
                tasks.push(ImportTask {
                    remote: remote_arc.clone(),
                    owner: owner_arc.clone(),
                    name: Arc::new(name),
                })
            }
        }
        drop(remote_arc);
        drop(owner_arc);

        if tasks.is_empty() {
            println!("Nothing to import");
            return Ok(());
        }

        println!("About to import:");
        for task in tasks.iter() {
            println!("  * {}", task.name);
        }
        let task_word = if tasks.len() == 1 {
            String::from("1 repo")
        } else {
            format!("{} repos", tasks.len())
        };
        confirm!("Do you want to import {}", task_word);

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

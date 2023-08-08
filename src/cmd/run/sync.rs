use std::collections::HashMap;
use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;
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
use crate::{config, confirm, utils};

/// Sync database and workspace.
#[derive(Args)]
pub struct SyncArgs {}

struct SyncTask {
    remote: Arc<Remote>,
    owner: String,

    name: String,

    path: PathBuf,

    show_name: String,
}

impl Task<()> for SyncTask {
    fn run(&self, rp: &Reporter<()>) -> Result<()> {
        let clone = match fs::read_dir(&self.path) {
            Ok(_) => false,
            Err(err) if err.kind() == ErrorKind::NotFound => true,
            Err(err) => {
                return Err(err).with_context(|| format!("Read dir {}", self.path.display()))
            }
        };

        let url = Repo::get_clone_url(self.owner.as_str(), self.name.as_str(), &self.remote);
        let path = format!("{}", self.path.display());
        if clone {
            rp.message(format!("Cloning {}...", self.show_name));
            Shell::exec_git_mute(&["clone", url.as_str(), path.as_str()])?;
        } else {
            rp.message(format!("Fetching {}...", self.show_name));
            Shell::exec_git_mute(&[
                "-C",
                path.as_str(),
                "remote",
                "set-url",
                "origin",
                url.as_str(),
            ])?;
            Shell::exec_git_mute(&["-C", path.as_str(), "fetch", "--all"])?;
        }
        if let Some(user) = &self.remote.user {
            Shell::exec_git_mute(&["-C", path.as_str(), "config", "user.name", user.as_str()])?;
        }
        if let Some(email) = &self.remote.email {
            Shell::exec_git_mute(&["-C", path.as_str(), "config", "user.email", email.as_str()])?;
        }
        Ok(())
    }

    fn message_done(&self, result: &Result<()>) -> String {
        match result {
            Ok(_) => format!("Sync {} done", self.show_name),
            Err(err) => format!("Sync {} error: {}", self.show_name, err),
        }
    }
}

impl Run for SyncArgs {
    fn run(&self) -> Result<()> {
        let mut db = Database::read()?;
        let workspace_repos = Repo::scan_workspace()?;

        let to_add: Vec<Rc<Repo>> = workspace_repos
            .into_iter()
            .filter(|repo| {
                if let Some(_) = db.get(
                    repo.remote.as_str(),
                    repo.owner.as_str(),
                    repo.name.as_str(),
                ) {
                    return false;
                }
                true
            })
            .collect();
        let mut added = false;
        if !to_add.is_empty() {
            println!("About to add missing repo:");
            for repo in to_add.iter() {
                println!("  * {}", repo.full_name());
            }
            confirm!(
                "Do you want to add {} to database",
                utils::plural(&to_add, "repo")
            );
            for repo in to_add {
                db.update(repo);
            }
            added = true;
        }

        let repos = db.list_all();
        let mut tasks = Vec::with_capacity(repos.len());
        let mut remotes: HashMap<String, Arc<Remote>> = HashMap::new();
        for repo in repos {
            let remote = match remotes.get(repo.remote.as_str()) {
                Some(remote) => remote.clone(),
                None => {
                    let remote = config::must_get_remote(repo.remote.as_str())?;
                    let remote = Arc::new(remote);
                    let ret = remote.clone();
                    remotes.insert(format!("{}", repo.remote), remote);
                    ret
                }
            };
            tasks.push(SyncTask {
                remote,
                owner: format!("{}", repo.owner),
                name: format!("{}", repo.name),
                path: repo.get_path(),
                show_name: repo.full_name(),
            });
        }

        let _ = batch::run("Sync", tasks);
        if added {
            db.close()?;
        }
        Ok(())
    }
}

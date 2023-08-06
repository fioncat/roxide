use std::fs;
use std::io::ErrorKind;
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use clap::Args;
use console::style;

use crate::cmd::Run;
use crate::config::types::Remote;
use crate::errors::SilentExit;
use crate::repo::database::Database;
use crate::repo::types::Repo;
use crate::shell::Shell;
use crate::{api, config, confirm, error, info, utils};

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

struct Task {
    remote_cfg: Arc<Remote>,
    remote: String,
    owner: String,

    name: String,
}

impl Task {
    fn handle(self) -> Result<Task> {
        match self._handle() {
            Ok(_) => Ok(self),
            Err(err) => match err.downcast::<SilentExit>() {
                Ok(SilentExit { code }) => {
                    error!(
                        "Import {} git command failed with code {}, please try to import manually",
                        code,
                        style(&self.name).cyan(),
                    );
                    bail!("git failed with code {}", code)
                }
                Err(err) => {
                    error!(
                        "Import {} failed: {}",
                        style(&self.name).cyan(),
                        style(&err).red()
                    );
                    Err(err)
                }
            },
        }
    }

    fn _handle(&self) -> Result<()> {
        let repo = Repo::new(&self.remote, &self.owner, &self.name, None);
        let path = repo.get_path();

        let display = style(&repo.name).cyan();

        match fs::read_dir(&path) {
            Ok(_) => {
                info!("Skip cloning {}", display);
                return Ok(());
            }
            Err(err) if err.kind() == ErrorKind::NotFound => {}
            Err(err) => return Err(err).with_context(|| format!("Read dir {}", path.display())),
        }

        let url = repo.clone_url(&self.remote_cfg);
        let path = format!("{}", path.display());

        Shell::git(&["clone", url.as_str(), path.as_str()])
            .piped_stderr()
            .with_desc(format!("Cloning {}...", display))
            .execute()?
            .check()?;

        if let Some(user) = &self.remote_cfg.user {
            Shell::git(&["-C", path.as_str(), "config", "user.name", user.as_str()])
                .piped_stderr()
                .set_mute(true)
                .execute()?
                .check()?;
        }
        if let Some(email) = &self.remote_cfg.email {
            Shell::git(&["-C", path.as_str(), "config", "user.email", email.as_str()])
                .set_mute(true)
                .execute()?
                .check()?;
        }
        Ok(())
    }
}

impl Run for ImportArgs {
    fn run(&self) -> Result<()> {
        let mut db = Database::read()?;
        let remote = config::must_get_remote(&self.remote)?;

        let provider = api::init_provider(&remote, self.force)?;
        let names = provider.list_repos(&self.owner)?;

        let remote = Arc::new(remote);

        let mut tasks = Vec::with_capacity(names.len());
        for name in names {
            if let None = db.get(&self.remote, &self.owner, &name) {
                tasks.push(Task {
                    remote_cfg: remote.clone(),
                    remote: self.remote.clone(),
                    owner: self.owner.clone(),
                    name,
                })
            }
        }
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

        let results = utils::run_async(Task::handle, tasks);
        let mut failed_count = 0;
        let mut suc_count = 0;
        for result in results {
            match result {
                Ok(task) => {
                    suc_count += 1;
                    db.update(Repo::new(task.remote, task.owner, task.name, None));
                }
                Err(_) => failed_count += 1,
            }
        }
        println!();
        info!(
            "Import done with {} successed, {} failed",
            style(suc_count).green(),
            style(failed_count).red(),
        );

        db.close()
    }
}

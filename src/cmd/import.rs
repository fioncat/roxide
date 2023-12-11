use std::sync::Arc;
use std::{fs, io};

use anyhow::{Context, Result};
use clap::Args;

use crate::batch::{self, Task};
use crate::cmd::{Completion, Run};
use crate::config::Config;
use crate::repo::database::{Database, SelectOptions, Selector};
use crate::repo::{Remote, Repo};
use crate::term::{self, Cmd, GitCmd};
use crate::{stderrln, utils};

/// Import repositories from remote in batches.
#[derive(Args)]
pub struct ImportArgs {
    /// Repository selection head.
    pub head: String,

    /// The owner to import.
    pub owner: String,

    /// When calling the remote API, ignore caches that are not expired.
    #[clap(short)]
    pub force: bool,

    /// Use editor to filter items before importing.
    #[clap(short)]
    pub edit: bool,

    /// Append these labels to the database.
    #[clap(short)]
    pub labels: Option<String>,
}

impl Run for ImportArgs {
    fn run(&self, cfg: &Config) -> Result<()> {
        let mut db = Database::load(cfg)?;

        let opts = SelectOptions::default()
            .with_force_search(self.force)
            .with_many_edit(self.edit);
        let head = Some(self.head.clone());
        let query = Some(self.owner.clone());
        let selector = Selector::from_args(&db, &head, &query, opts);

        let (remote, owner_name, names) = selector.many_remote()?;
        if names.is_empty() {
            stderrln!("No repo to import");
            return Ok(());
        }
        term::must_confirm_items(&names, "import", "import", "Repo", "Repos")?;

        let remote_arc = Arc::new(remote.clone());
        let owner_arc = Arc::new(owner_name.clone());
        let cfg_arc = Arc::new(cfg.clone());

        let mut tasks = Vec::with_capacity(names.len());
        for name in names {
            tasks.push((
                name.clone(),
                ImportTask {
                    cfg: Arc::clone(&cfg_arc),
                    remote: Arc::clone(&remote_arc),
                    owner: Arc::clone(&owner_arc),
                    name: Arc::new(name),
                },
            ))
        }

        let labels = utils::parse_labels(&self.labels);

        let names = batch::must_run("Import", tasks)?;
        for name in names {
            let name = Arc::try_unwrap(name).unwrap();
            let repo = Repo::new(cfg, &remote.name, &owner_name, name, None, &labels)?;
            db.update(repo, None);
        }

        db.save()
    }
}

impl ImportArgs {
    pub fn completion() -> Completion {
        Completion {
            args: Completion::owner_args,
            flags: Some(Completion::labels),
        }
    }
}

struct ImportTask {
    cfg: Arc<Config>,

    remote: Arc<Remote>,
    owner: Arc<String>,

    name: Arc<String>,
}

impl Task<Arc<String>> for ImportTask {
    fn run(&self) -> Result<Arc<String>> {
        let path = Repo::get_workspace_path(
            &self.cfg,
            self.remote.name.as_str(),
            self.owner.as_str(),
            self.name.as_str(),
        );

        match fs::read_dir(&path) {
            Ok(_) => {
                return Ok(Arc::clone(&self.name));
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => {}
            Err(err) => return Err(err).with_context(|| format!("read dir {}", path.display())),
        }

        let url = Repo::get_clone_url(self.owner.as_str(), self.name.as_str(), &self.remote);
        let path = format!("{}", path.display());

        Cmd::git(&["clone", url.as_str(), path.as_str()]).execute_check()?;

        let git = GitCmd::with_path(path.as_str());
        if let Some(user) = &self.remote.cfg.user {
            git.exec(&["config", "user.name", user.as_str()])?;
        }
        if let Some(email) = &self.remote.cfg.email {
            git.exec(&["config", "user.email", email.as_str()])?;
        }
        Ok(Arc::clone(&self.name))
    }
}

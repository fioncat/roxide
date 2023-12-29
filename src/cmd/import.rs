use std::borrow::Cow;
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
        let selector = Selector::from_args(&head, &query, opts);

        let (remote, owner, names) = selector.many_remote(&db)?;
        if names.is_empty() {
            stderrln!("No repo to import");
            return Ok(());
        }
        term::must_confirm_items(&names, "import", "import", "Repo", "Repos")?;

        let cfg_arc = Arc::new(cfg.clone());

        let mut tasks = Vec::with_capacity(names.len());
        let mut repos = Vec::with_capacity(names.len());
        let labels = utils::parse_labels(&self.labels);
        for name in names {
            let mut repo = Repo::new(
                cfg,
                Cow::Borrowed(&remote),
                Cow::Borrowed(&owner),
                Cow::Owned(name),
                None,
            )?;
            repo.append_labels(labels.clone());
            let repo = Arc::new(repo);
            repos.push(Arc::clone(&repo));
            tasks.push((
                name.clone(),
                ImportTask {
                    cfg: Arc::clone(&cfg_arc),
                    repo,
                },
            ))
        }

        batch::must_run("Import", tasks)?;

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

struct ImportTask<'a> {
    cfg: Arc<Config>,
    repo: Arc<Repo<'a>>,
}

impl Task<()> for ImportTask<'_> {
    fn run(&self) -> Result<()> {
        let path = self.repo.get_path(&self.cfg);

        match fs::read_dir(&path) {
            Ok(_) => return Ok(()),
            Err(err) if err.kind() == io::ErrorKind::NotFound => {}
            Err(err) => return Err(err).with_context(|| format!("read dir {}", path.display())),
        }

        let url = self.repo.clone_url();
        let path = format!("{}", path.display());

        Cmd::git(&["clone", url.as_str(), path.as_str()]).execute_check()?;

        let git = GitCmd::with_path(path.as_str());
        if let Some(user) = &self.repo.remote_cfg.user {
            git.exec(&["config", "user.name", user.as_str()])?;
        }
        if let Some(email) = &self.repo.remote_cfg.email {
            git.exec(&["config", "user.email", email.as_str()])?;
        }
        Ok(())
    }
}

use std::fs;
use std::io;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Args;

use crate::batch::Task;
use crate::cmd::{Completion, Run};
use crate::config::Config;
use crate::repo::database::{Database, SelectOptions, Selector};
use crate::repo::Repo;
use crate::term::Cmd;
use crate::workflow::Workflow;
use crate::{api, confirm, utils};

/// Enter a repository.
#[derive(Args)]
pub struct HomeArgs {
    /// Repository selection head.
    pub head: Option<String>,

    /// Repository selection query.
    pub query: Option<String>,

    /// Use search instead of fuzzy matching.
    #[clap(short)]
    pub search: bool,

    /// When calling the remote API, ignore caches that are not expired.
    #[clap(short)]
    pub force: bool,

    /// Open repo in default browser rather than clone it.
    #[clap(short)]
    pub open: bool,

    /// Append these labels to the database.
    #[clap(short)]
    pub labels: Option<String>,
}

impl Run for HomeArgs {
    fn run(&self, cfg: &Config) -> Result<()> {
        let mut db = Database::load(cfg)?;

        let append_labels = utils::parse_labels(&self.labels);

        let opts = SelectOptions::default()
            .with_force_search(self.search)
            .with_force_no_cache(self.force);
        let selector = Selector::from_args(&self.head, &self.query, opts);

        let (mut repo, exists) = selector.one(&db)?;

        if self.open {
            let provider = api::build_provider(&cfg, &repo.remote_cfg, self.force)?;
            let api_repo = provider.get_repo(&repo.owner, &repo.name)?;
            return utils::open_url(&api_repo.web_url);
        }

        if !exists {
            confirm!("Do you want to create {}", repo.name_with_owner());
        }

        let path = repo.get_path(cfg);
        match fs::read_dir(&path) {
            Ok(_) => {}
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                self.create_dir(&cfg, &repo, &path)?;
            }
            Err(err) => {
                return Err(err).with_context(|| format!("read repo directory {}", path.display()));
            }
        }

        println!("{}", path.display());

        repo.append_labels(append_labels);
        repo.accessed += 1;
        repo.last_accessed = cfg.now();
        db.upsert(repo.update());
        db.save()
    }
}

impl HomeArgs {
    fn create_dir(&self, cfg: &Config, repo: &Repo, path: &PathBuf) -> Result<()> {
        if let Some(_) = repo.remote_cfg.clone {
            return self.clone(repo, path);
        }

        fs::create_dir_all(path)
            .with_context(|| format!("create repo directory {}", path.display()))?;
        let path = format!("{}", path.display());
        Cmd::git(&["-C", path.as_str(), "init"])
            .with_display("Git init")
            .execute()?;
        if let Some(owner) = repo.remote_cfg.owners.get(repo.owner.as_ref()) {
            if let Some(workflow_names) = &owner.on_create {
                for workflow_name in workflow_names.iter() {
                    let wf = Workflow::load(workflow_name, cfg, repo)?;
                    wf.run()?;
                }
            }
        }

        Ok(())
    }

    fn clone(&self, repo: &Repo, path: &PathBuf) -> Result<()> {
        let url = repo.clone_url();
        let path = format!("{}", path.display());
        Cmd::git(&["clone", url.as_str(), path.as_str()])
            .with_display(format!("Clone {}", repo.name_with_labels()))
            .execute()?;

        if let Some(user) = &repo.remote_cfg.user {
            Cmd::git(&["-C", path.as_str(), "config", "user.name", user.as_str()])
                .with_display(format!("Set user to {}", user))
                .execute()?;
        }
        if let Some(email) = &repo.remote_cfg.email {
            Cmd::git(&["-C", path.as_str(), "config", "user.email", email.as_str()])
                .with_display(format!("Set email to {}", email))
                .execute()?;
        }
        Ok(())
    }

    pub fn completion() -> Completion {
        Completion {
            args: Completion::repo_args,
            flags: Some(Completion::labels),
        }
    }
}

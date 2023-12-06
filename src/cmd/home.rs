use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::rc::Rc;

use anyhow::{Context, Result};
use clap::Args;

use crate::cmd::{BaseCompletion, Completion, CompletionResult, Run};
use crate::config::Config;
use crate::repo::database::{Database, SelectOptions, Selector};
use crate::repo::Repo;
use crate::term::{Cmd, Workflow};
use crate::{api, confirm, utils};

/// Enter the home path of a repo.
#[derive(Args)]
pub struct HomeArgs {
    pub head: Option<String>,

    pub query: Option<String>,

    /// If true, use search instead of fuzzy matching.
    #[clap(short)]
    pub search: bool,

    /// If true, the cache will not be used when calling the API search.
    #[clap(short)]
    pub force: bool,

    /// Open repo in default browser rather than clone it.
    #[clap(short)]
    pub open: bool,
}

impl Run for HomeArgs {
    fn run(&self) -> Result<()> {
        let cfg = Config::load()?;
        let mut db = Database::load(&cfg)?;

        let opts = SelectOptions::default()
            .with_force_search(self.search)
            .with_force_no_cache(self.force);
        let selector = Selector::from_args(&db, &self.head, &self.query, opts);

        let (repo, exists) = selector.one()?;

        if self.open {
            let provider = api::build_provider(&cfg, &repo.remote, self.force)?;
            let api_repo = provider.get_repo(&repo.owner.name, &repo.name)?;
            return utils::open_url(&api_repo.web_url);
        }

        if !exists {
            confirm!("Do you want to create {}", repo.name_with_owner());
        }

        let path = repo.get_path(&cfg);
        match fs::read_dir(&path) {
            Ok(_) => {}
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                self.create_dir(&cfg, &repo, &path)?;
            }
            Err(err) => {
                return Err(err).with_context(|| format!("read repo directory {}", path.display()));
            }
        }

        writeln!(io::stdout(), "{}", path.display()).context("write repo path to stdout")?;

        db.update(repo, None);

        db.save()?;
        Ok(())
    }
}

impl HomeArgs {
    fn create_dir(&self, cfg: &Config, repo: &Rc<Repo>, path: &PathBuf) -> Result<()> {
        if let Some(_) = repo.remote.cfg.clone {
            return self.clone(repo, path);
        }

        fs::create_dir_all(path)
            .with_context(|| format!("create repo directory {}", path.display()))?;
        let path = format!("{}", path.display());
        Cmd::git(&["-C", path.as_str(), "init"])
            .with_display("Git init")
            .execute_check()?;
        if let Some(owner) = repo.remote.cfg.owners.get(repo.owner.name.as_str()) {
            if let Some(workflow_names) = &owner.on_create {
                for workflow_name in workflow_names.iter() {
                    let wf = Workflow::new(cfg, workflow_name)?;
                    wf.execute_repo(repo)?;
                }
            }
        }

        Ok(())
    }

    fn clone(&self, repo: &Rc<Repo>, path: &PathBuf) -> Result<()> {
        let url = repo.clone_url();
        let path = format!("{}", path.display());
        Cmd::git(&["clone", url.as_str(), path.as_str()])
            .with_display(format!("Clone {}", repo.name_with_labels()))
            .execute_check()?;

        if let Some(user) = &repo.remote.cfg.user {
            Cmd::git(&["-C", path.as_str(), "config", "user.name", user.as_str()])
                .with_display(format!("Set user to {}", user))
                .execute_check()?;
        }
        if let Some(email) = &repo.remote.cfg.email {
            Cmd::git(&["-C", path.as_str(), "config", "user.email", email.as_str()])
                .with_display(format!("Set email to {}", email))
                .execute_check()?;
        }
        Ok(())
    }
}

pub struct HomeCompletion {
    base: BaseCompletion,
}

impl HomeCompletion {
    pub fn load() -> Result<Box<dyn Completion>> {
        let base = BaseCompletion::load()?;
        Ok(Box::new(HomeCompletion { base }))
    }
}

impl Completion for HomeCompletion {
    fn args(&self, args: &[&str]) -> Result<CompletionResult> {
        self.base.repo_args(args)
    }

    fn flag(&self, _flag: &str) -> Result<CompletionResult> {
        todo!()
    }
}

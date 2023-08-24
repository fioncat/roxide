use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::rc::Rc;

use anyhow::{Context, Result};
use clap::Args;

use crate::cmd::Run;
use crate::config::types::Remote;
use crate::confirm;
use crate::repo::database::Database;
use crate::repo::query::Query;
use crate::repo::types::Repo;
use crate::shell::{Shell, Workflow};

/// Print the home path of a repo, recommand to use `zz` command instead.
#[derive(Args)]
pub struct HomeArgs {
    /// The repo query, format is `[remote] [owner[/[name]]]`
    pub query: Vec<String>,

    /// If true, use search instead of fuzzy matching.
    #[clap(long, short)]
    pub search: bool,

    /// If true, the cache will not be used when calling the API search.
    #[clap(long, short)]
    pub force: bool,
}

impl Run for HomeArgs {
    fn run(&self) -> Result<()> {
        let mut db = Database::read()?;
        let query = Query::new(&db, self.query.clone())
            .with_force(self.force)
            .with_search(self.search);

        let result = query.one()?;
        if !result.exists {
            confirm!(
                "Do you want to create {}/{}",
                result.repo.owner.as_ref(),
                result.repo.name.as_ref()
            );
        }
        let (remote, repo) = result.as_repo();

        let dir = repo.get_path();
        match fs::read_dir(&dir) {
            Ok(_) => {}
            Err(err) if err.kind() == ErrorKind::NotFound => {
                self.create_dir(&remote, &repo, &dir)?;
            }
            Err(err) => {
                return Err(err).with_context(|| format!("Read repo directory {}", dir.display()));
            }
        }

        println!("{}", dir.display());
        db.update(repo);

        db.close()
    }
}

impl HomeArgs {
    fn create_dir(&self, remote: &Remote, repo: &Rc<Repo>, dir: &PathBuf) -> Result<()> {
        if let Some(_) = remote.clone {
            return self.clone(remote, repo, dir);
        }
        fs::create_dir_all(dir)
            .with_context(|| format!("Create repo directory {}", dir.display()))?;
        let path = format!("{}", dir.display());
        Shell::git(&["-C", path.as_str(), "init"])
            .with_desc("Git init")
            .execute()?
            .check()?;
        if let Some(owner) = remote.owners.get(repo.owner.as_str()) {
            if let Some(workflow_names) = &owner.on_create {
                for workflow_name in workflow_names.iter() {
                    let wf = Workflow::new(workflow_name)?;
                    wf.execute_repo(repo)?;
                }
            }
        }

        Ok(())
    }

    fn clone(&self, remote: &Remote, repo: &Rc<Repo>, dir: &PathBuf) -> Result<()> {
        let url = repo.clone_url(&remote);
        let path = format!("{}", dir.display());
        Shell::git(&["clone", url.as_str(), path.as_str()])
            .with_desc(format!("Clone {}", repo.full_name()))
            .execute()?
            .check()?;

        if let Some(user) = &remote.user {
            Shell::git(&["-C", path.as_str(), "config", "user.name", user.as_str()])
                .with_desc(format!("Set user to {}", user))
                .execute()?
                .check()?;
        }
        if let Some(email) = &remote.email {
            Shell::git(&["-C", path.as_str(), "config", "user.email", email.as_str()])
                .with_desc(format!("Set email to {}", email))
                .execute()?
                .check()?;
        }
        Ok(())
    }
}

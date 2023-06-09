use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::rc::Rc;

use anyhow::{bail, Context, Result};
use roxide::config;
use roxide::config::types::Remote;
use roxide::repo::context::RepoContext;
use roxide::repo::database::Database;
use roxide::repo::types::Repo;
use roxide::shell::Shell;
use roxide::{info, shell};

use crate::cmd::app::HomeArgs;
use crate::cmd::Run;

impl Run for HomeArgs {
    fn run(&self) -> Result<()> {
        let mut db = Database::read()?;
        let ctx = RepoContext::query(&self.query, &db, self.search)?;

        let dir = ctx.repo.get_path();
        match fs::read_dir(&dir) {
            Ok(_) => {}
            Err(err) if err.kind() == ErrorKind::NotFound => {
                self.create_dir(&ctx.remote, &ctx.repo, &dir)?;
            }
            Err(err) => {
                return Err(err).with_context(|| format!("Read repo directory {}", dir.display()));
            }
        }

        println!("{}", dir.display());
        db.update(ctx.repo);

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
            .execute()?;
        if let Some(owner) = remote.owners.get(repo.owner.as_str()) {
            if let Some(workflow_names) = &owner.on_create {
                for workflow_name in workflow_names.iter() {
                    let maybe_workflow = config::base().workflows.get(workflow_name);
                    if let None = maybe_workflow {
                        bail!(
                            "Could not find workeflow {} for owner {}, please check your config",
                            workflow_name,
                            repo.owner.as_str(),
                        );
                    }
                    let workflow = maybe_workflow.unwrap();
                    info!("Execute on_create workflow {}", workflow_name);
                    shell::execute_workflow(workflow, repo)?;
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
            .execute()?;

        if let Some(user) = &remote.user {
            Shell::git(&["-C", path.as_str(), "config", "user.name", user.as_str()])
                .with_desc(format!("Set user to {}", user))
                .execute()?;
        }
        if let Some(email) = &remote.email {
            Shell::git(&["-C", path.as_str(), "config", "user.email", email.as_str()])
                .with_desc(format!("Set email to {}", email))
                .execute()?;
        }
        Ok(())
    }
}

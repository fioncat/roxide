use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::rc::Rc;

use anyhow::{bail, Context, Result};
use clap::Args;

use crate::cmd::Run;
use crate::config::types::Remote;
use crate::repo::database::Database;
use crate::repo::types::{NameLevel, Repo};
use crate::shell::Shell;
use crate::{api, info, shell};
use crate::{config, confirm, utils};

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
        let (remote, repo) = self.query_repo(&db)?;

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
    fn query_repo(&self, db: &Database) -> Result<(Remote, Rc<Repo>)> {
        match self.query.len() {
            0 => {
                let repo = if self.search {
                    self.search_local(db.list_all(), NameLevel::Full)?
                } else {
                    db.must_latest("")?
                };
                let remote = config::must_get_remote(repo.remote.as_str())?;
                Ok((remote, repo))
            }
            1 => {
                let maybe_remote = &self.query[0];
                match config::get_remote(maybe_remote)? {
                    Some(remote) => {
                        let repo = if self.search {
                            self.search_local(db.list_by_remote(&remote.name), NameLevel::Owner)?
                        } else {
                            db.must_latest(&remote.name)?
                        };
                        let remote = config::must_get_remote(repo.remote.as_str())?;
                        Ok((remote, repo))
                    }
                    None => {
                        let repo = db.must_get_fuzzy("", maybe_remote)?;
                        let remote = config::must_get_remote(repo.remote.as_str())?;
                        Ok((remote, repo))
                    }
                }
            }
            _ => {
                let remote_name = &self.query[0];
                let remote = config::must_get_remote(remote_name)?;
                let query = &self.query[1];
                if query.ends_with("/") {
                    let mut owner = query.strip_suffix("/").unwrap();
                    if let Some(raw_owner) = remote.owner_alias.get(owner) {
                        owner = raw_owner.as_str();
                    }
                    let repo = self.search_api(db, &remote, &owner)?;
                    return Ok((remote, repo));
                }

                let (owner, name) = utils::parse_query(&remote, query);
                if owner.is_empty() {
                    let repo = db.must_get_fuzzy(&remote.name, &name)?;
                    return Ok((remote, repo));
                }

                Ok((remote, self.get_repo(db, remote_name, &owner, &name)?))
            }
        }
    }

    fn get_repo<S>(&self, db: &Database, remote: S, owner: S, name: S) -> Result<Rc<Repo>>
    where
        S: AsRef<str>,
    {
        match db.get(remote.as_ref(), owner.as_ref(), name.as_ref()) {
            Some(repo) => Ok(repo),
            None => {
                confirm!("Do you want to create {}/{}", owner.as_ref(), name.as_ref());
                let repo = Repo::new(remote.as_ref(), owner.as_ref(), name.as_ref(), None);
                Ok(repo)
            }
        }
    }

    fn search_local(&self, vec: Vec<Rc<Repo>>, level: NameLevel) -> Result<Rc<Repo>> {
        let items: Vec<_> = vec.iter().map(|repo| repo.as_string(&level)).collect();
        let idx = shell::search(&items)?;
        let repo = Rc::clone(&vec[idx]);
        Ok(repo)
    }

    fn search_api(&self, db: &Database, remote: &Remote, owner: &str) -> Result<Rc<Repo>> {
        match remote.provider {
            Some(_) => {
                let provider = api::init_provider(remote, self.force)?;
                let api_repos = provider.list_repos(owner)?;
                let idx = shell::search(&api_repos)?;
                let name = &api_repos[idx];
                self.get_repo(db, remote.name.as_str(), owner, name.as_str())
            }
            None => self.search_local(
                db.list_by_owner(remote.name.as_str(), owner),
                NameLevel::Name,
            ),
        }
    }

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

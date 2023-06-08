use std::fs;
use std::io::ErrorKind;
use std::rc::Rc;

use anyhow::{Context, Result};
use roxide::config::types::Remote;
use roxide::repo::database::Database;
use roxide::repo::types::{NameLevel, Repo};
use roxide::shell::Shell;
use roxide::{api, info, shell};
use roxide::{config, confirm, utils};

use crate::cmd::app::HomeArgs;
use crate::cmd::Run;

impl Run for HomeArgs {
    fn run(&self) -> Result<()> {
        let mut db = Database::read()?;
        let (remote, repo) = self.query_repo(&db)?;

        let dir = repo.get_path();
        match fs::read_dir(&dir) {
            Ok(_) => {}
            Err(err) if err.kind() == ErrorKind::NotFound => {
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
            }
            Err(err) => {
                return Err(err).with_context(|| format!("Read repo directory {}", dir.display()))
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
                    let owner = query.strip_suffix("/").unwrap();
                    let owner = remote.replace_owner(owner.to_string());
                    let repo = self.search_api(db, &remote, &owner)?;
                    return Ok((remote, repo));
                }

                let (owner, name) = utils::parse_query(query);
                if owner.is_empty() {
                    let repo = db.must_get_fuzzy(&remote.name, &name)?;
                    return Ok((remote, repo));
                }

                let owner = remote.replace_owner(owner);
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
                let provider = api::init_provider(remote)?;
                info!("Search for owner {}", owner);
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
}

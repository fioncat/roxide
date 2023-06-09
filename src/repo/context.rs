use std::rc::Rc;

use anyhow::Result;

use crate::api;
use crate::config;
use crate::config::types::Remote;
use crate::repo::types::NameLevel;
use crate::shell;
use crate::utils;
use crate::{confirm, info};

use super::database::Database;
use super::types::Repo;

pub struct RepoContext {
    pub remote: Remote,
    pub repo: Rc<Repo>,
}

impl RepoContext {
    pub fn new(remote: Remote, repo: Rc<Repo>) -> RepoContext {
        RepoContext { remote, repo }
    }

    pub fn query(args: &[String], db: &Database, search: bool) -> Result<RepoContext> {
        match args.len() {
            0 => {
                let repo = if search {
                    Self::search_local(db.list_all(), NameLevel::Full)?
                } else {
                    db.must_latest("")?
                };
                let remote = config::must_get_remote(repo.remote.as_str())?;
                Ok(Self::new(remote, repo))
            }
            1 => {
                let maybe_remote = &args[0];
                match config::get_remote(maybe_remote)? {
                    Some(remote) => {
                        let repo = if search {
                            Self::search_local(db.list_by_remote(&remote.name), NameLevel::Owner)?
                        } else {
                            db.must_latest(&remote.name)?
                        };
                        let remote = config::must_get_remote(repo.remote.as_str())?;
                        Ok(Self::new(remote, repo))
                    }
                    None => {
                        let repo = db.must_get_fuzzy("", maybe_remote)?;
                        let remote = config::must_get_remote(repo.remote.as_str())?;
                        Ok(Self::new(remote, repo))
                    }
                }
            }
            _ => {
                let remote_name = &args[0];
                let remote = config::must_get_remote(remote_name)?;
                let query = &args[1];
                if query.ends_with("/") {
                    let owner = query.strip_suffix("/").unwrap();
                    let owner = remote.replace_owner(owner.to_string());
                    let repo = Self::search_api(db, &remote, &owner)?;
                    return Ok(Self::new(remote, repo));
                }

                let (owner, name) = utils::parse_query(query);
                if owner.is_empty() {
                    let repo = db.must_get_fuzzy(&remote.name, &name)?;
                    return Ok(Self::new(remote, repo));
                }

                let owner = remote.replace_owner(owner);
                Ok(Self::new(
                    remote,
                    Self::get_repo(db, remote_name, &owner, &name)?,
                ))
            }
        }
    }

    fn get_repo<S>(db: &Database, remote: S, owner: S, name: S) -> Result<Rc<Repo>>
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

    fn search_local(vec: Vec<Rc<Repo>>, level: NameLevel) -> Result<Rc<Repo>> {
        let items: Vec<_> = vec.iter().map(|repo| repo.as_string(&level)).collect();
        let idx = shell::search(&items)?;
        let repo = Rc::clone(&vec[idx]);
        Ok(repo)
    }

    fn search_api(db: &Database, remote: &Remote, owner: &str) -> Result<Rc<Repo>> {
        match remote.provider {
            Some(_) => {
                let provider = api::init_provider(remote)?;
                info!("Search for owner {}", owner);
                let api_repos = provider.list_repos(owner)?;
                let idx = shell::search(&api_repos)?;
                let name = &api_repos[idx];
                Self::get_repo(db, remote.name.as_str(), owner, name.as_str())
            }
            None => Self::search_local(
                db.list_by_owner(remote.name.as_str(), owner),
                NameLevel::Name,
            ),
        }
    }
}

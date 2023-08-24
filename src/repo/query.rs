use std::collections::HashSet;
use std::rc::Rc;

use anyhow::{bail, Result};

use crate::config::types::Remote;
use crate::repo::database::Database;
use crate::repo::types::{NameLevel, Repo};
use crate::{api, config, shell, utils};

pub struct Query<'a> {
    db: &'a Database,

    query: Vec<String>,

    search: bool,
    force: bool,

    local_only: bool,
    remote_only: bool,

    repo_path: Option<String>,

    filter: bool,
}

pub struct QueryOneResult {
    pub remote: Remote,
    pub repo: Rc<Repo>,
    pub exists: bool,
}

impl QueryOneResult {
    pub fn as_repo(self) -> (Remote, Rc<Repo>) {
        (self.remote, self.repo)
    }
}

pub struct QueryManyResult {
    remote: Option<Remote>,
    names: Option<Vec<String>>,

    repos: Option<Vec<Rc<Repo>>>,

    level: NameLevel,
}

impl QueryManyResult {
    pub fn level(&self) -> NameLevel {
        self.level.clone()
    }

    pub fn as_local(self) -> Vec<Rc<Repo>> {
        self.repos.unwrap()
    }

    pub fn as_remote(self) -> (Remote, Vec<String>) {
        (self.remote.unwrap(), self.names.unwrap())
    }
}

impl<'a> Query<'_> {
    pub fn from_args(
        db: &'a Database,
        remote: &Option<String>,
        query: &Option<String>,
    ) -> Query<'a> {
        let mut args = vec![];
        if let Some(remote) = remote.as_ref() {
            args.push(remote.clone());
        }
        if let Some(query) = query.as_ref() {
            args.push(query.clone());
        }
        Self::new(db, args)
    }

    pub fn new(db: &Database, query: Vec<String>) -> Query {
        Query {
            db,
            query,
            search: false,
            force: false,
            local_only: false,
            remote_only: false,
            repo_path: None,
            filter: false,
        }
    }

    pub fn with_force(mut self, force: bool) -> Self {
        self.force = force;
        self
    }

    pub fn with_search(mut self, search: bool) -> Self {
        self.search = search;
        self
    }

    pub fn with_remote_only(mut self, val: bool) -> Self {
        self.remote_only = val;
        self
    }

    pub fn with_local_only(mut self, val: bool) -> Self {
        self.local_only = val;
        self
    }

    pub fn with_repo_path(mut self, path: String) -> Self {
        self.repo_path = Some(path);
        self
    }

    pub fn with_filter(mut self, filter: bool) -> Self {
        self.filter = filter;
        self
    }

    pub fn one(&self) -> Result<QueryOneResult> {
        if self.query.is_empty() {
            if self.remote_only {
                unreachable!();
            }
            let repo = if self.search {
                self.search_local(self.db.list_all(), NameLevel::Full)?
            } else {
                self.db.must_latest("")?
            };
            let remote = config::must_get_remote(repo.remote.as_str())?;
            return Ok(QueryOneResult {
                remote,
                repo,
                exists: true,
            });
        }

        if self.query.len() == 1 {
            if self.remote_only {
                unreachable!();
            }
            let maybe_remote = &self.query[0];
            return match config::get_remote(maybe_remote)? {
                Some(remote) => {
                    let repo = if self.search {
                        self.search_local(self.db.list_by_remote(&remote.name), NameLevel::Owner)?
                    } else {
                        self.db.must_latest(&remote.name)?
                    };
                    let remote = config::must_get_remote(repo.remote.as_str())?;
                    Ok(QueryOneResult {
                        remote,
                        repo,
                        exists: true,
                    })
                }
                None => {
                    let repo = self.db.must_get_fuzzy("", maybe_remote)?;
                    let remote = config::must_get_remote(repo.remote.as_str())?;
                    Ok(QueryOneResult {
                        remote,
                        repo,
                        exists: true,
                    })
                }
            };
        }

        let remote_name = &self.query[0];
        let remote = config::must_get_remote(remote_name)?;
        let query = &self.query[1];
        if query.ends_with("/") {
            let owner = query.strip_suffix("/").unwrap();
            if self.remote_only {
                let provider = api::init_provider(&remote, self.force)?;
                let items = provider.list_repos(owner)?;

                let attached = self.db.list_by_owner(remote.name.as_str(), owner);
                let attached_set: HashSet<&str> =
                    attached.iter().map(|repo| repo.name.as_str()).collect();

                let mut items: Vec<_> = items
                    .into_iter()
                    .filter(|name| !attached_set.contains(name.as_str()))
                    .collect();
                let idx = shell::search(&items)?;
                let name = items.remove(idx);
                return Ok(self.get_repo(remote, owner, name.as_str()));
            }

            let mut search_local = self.local_only;
            if let None = remote.provider {
                search_local = true;
            }
            if search_local {
                let repo = self.search_local(
                    self.db.list_by_owner(remote.name.as_str(), owner),
                    NameLevel::Name,
                )?;
                return Ok(QueryOneResult {
                    remote,
                    repo,
                    exists: true,
                });
            }

            let provider = api::init_provider(&remote, self.force)?;
            let api_repos = provider.list_repos(owner)?;
            let idx = shell::search(&api_repos)?;
            let name = &api_repos[idx];
            return Ok(self.get_repo(remote, owner, name.as_str()));
        }

        let (owner, name) = parse_owner(query);
        if owner.is_empty() {
            if self.remote_only {
                bail!("Invalid args, the repo name should contain owner");
            }
            let repo = self.db.must_get_fuzzy(&remote.name, &name)?;
            return Ok(QueryOneResult {
                remote,
                repo,
                exists: true,
            });
        }
        return Ok(self.get_repo(remote, owner, name));
    }

    pub fn must_one(&self) -> Result<(Remote, Rc<Repo>)> {
        let result = self.one()?;
        if !result.exists {
            bail!("Could not find repo {}", result.repo.full_name());
        }
        Ok(result.as_repo())
    }

    fn get_repo<S>(&self, remote: Remote, owner: S, name: S) -> QueryOneResult
    where
        S: AsRef<str>,
    {
        match self
            .db
            .get(remote.name.as_str(), owner.as_ref(), name.as_ref())
        {
            Some(repo) => QueryOneResult {
                remote,
                repo,
                exists: true,
            },
            None => {
                let remote_name = remote.name.clone();
                let repo = Repo::new(
                    remote_name,
                    owner.as_ref().to_string(),
                    name.as_ref().to_string(),
                    self.repo_path.clone(),
                );
                QueryOneResult {
                    remote,
                    repo,
                    exists: false,
                }
            }
        }
    }

    fn search_local(&self, vec: Vec<Rc<Repo>>, level: NameLevel) -> Result<Rc<Repo>> {
        let items: Vec<_> = vec.iter().map(|repo| repo.as_string(&level)).collect();
        let idx = shell::search(&items)?;
        let repo = Rc::clone(&vec[idx]);
        Ok(repo)
    }

    pub fn many(&self) -> Result<QueryManyResult> {
        let mut result = self.list_many()?;
        if self.remote_only {
            if result.names.as_ref().unwrap().is_empty() {
                return Ok(result);
            }
            if self.filter {
                let names = utils::edit_items(result.names.clone().unwrap())?;
                result.names = Some(names);
            }
            return Ok(result);
        }

        if self.filter {
            let items: Vec<String> = result
                .repos
                .as_ref()
                .unwrap()
                .iter()
                .map(|repo| repo.as_string(&result.level))
                .collect();
            let items = utils::edit_items(items)?;
            let items_set: HashSet<String> = items.into_iter().collect();

            let QueryManyResult {
                remote: _,
                names: _,

                repos,
                level,
            } = result;

            let repos: Vec<Rc<Repo>> = repos
                .unwrap()
                .into_iter()
                .filter(|repo| items_set.contains(&repo.as_string(&level)))
                .collect();

            return Ok(QueryManyResult {
                remote: None,
                names: None,

                repos: Some(repos),
                level,
            });
        }

        Ok(result)
    }

    fn list_many(&self) -> Result<QueryManyResult> {
        if self.query.is_empty() {
            if self.remote_only {
                unreachable!();
            }
            return Ok(QueryManyResult {
                remote: None,
                names: None,

                repos: Some(self.db.list_all()),
                level: NameLevel::Full,
            });
        }

        if self.query.len() == 1 {
            if self.remote_only {
                unreachable!();
            }
            let maybe_remote = &self.query[0];
            return match config::get_remote(maybe_remote)? {
                Some(remote) => {
                    let repos = self.db.list_by_remote(&remote.name);
                    Ok(QueryManyResult {
                        remote: None,
                        names: None,

                        repos: Some(repos),
                        level: NameLevel::Owner,
                    })
                }
                None => {
                    let repo = self.db.must_get_fuzzy("", maybe_remote)?;
                    Ok(QueryManyResult {
                        remote: None,
                        names: None,

                        repos: Some(vec![repo]),
                        level: NameLevel::Owner,
                    })
                }
            };
        }
        let remote_name = &self.query[0];
        let remote = config::must_get_remote(remote_name)?;
        let query = &self.query[1];
        let owner = match query.strip_suffix("/") {
            Some(owner) => owner,
            None => query.as_str(),
        };
        if self.remote_only {
            let provider = api::init_provider(&remote, self.force)?;
            let items = provider.list_repos(owner)?;

            let attached = self.db.list_by_owner(remote.name.as_str(), owner);
            let attached_set: HashSet<&str> =
                attached.iter().map(|repo| repo.name.as_str()).collect();

            let names: Vec<_> = items
                .into_iter()
                .filter(|name| !attached_set.contains(name.as_str()))
                .collect();
            return Ok(QueryManyResult {
                remote: Some(remote),
                names: Some(names),

                repos: None,
                level: NameLevel::Name,
            });
        }

        Ok(QueryManyResult {
            remote: None,
            names: None,

            repos: Some(self.db.list_by_owner(remote.name.as_str(), owner)),
            level: NameLevel::Name,
        })
    }
}

fn parse_owner(query: impl AsRef<str>) -> (String, String) {
    let items: Vec<_> = query.as_ref().split("/").collect();
    let items_len = items.len();
    let mut group_buffer: Vec<String> = Vec::with_capacity(items_len - 1);
    let mut base = "";
    for (idx, item) in items.iter().enumerate() {
        if idx == items_len - 1 {
            base = item;
        } else {
            group_buffer.push(item.to_string());
        }
    }
    (group_buffer.join("/"), base.to_string())
}

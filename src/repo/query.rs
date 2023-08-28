use std::collections::HashSet;
use std::rc::Rc;

use anyhow::{bail, Result};

use crate::config::types::Remote;
use crate::repo::database::Database;
use crate::repo::types::{NameLevel, Repo};
use crate::{api, config, shell, utils};

pub struct SelectOptions {
    pub search: bool,
    pub force: bool,

    pub repo_path: Option<String>,

    pub local_only: bool,
}

impl SelectOptions {
    pub fn new() -> Self {
        SelectOptions {
            search: false,
            force: false,
            repo_path: None,
            local_only: false,
        }
    }

    pub fn with_search(mut self, val: bool) -> Self {
        self.search = val;
        self
    }

    pub fn with_force(mut self, val: bool) -> Self {
        self.force = val;
        self
    }

    pub fn with_repo_path(mut self, path: String) -> Self {
        self.repo_path = Some(path);
        self
    }

    pub fn with_local_only(mut self, val: bool) -> Self {
        self.local_only = val;
        self
    }
}

pub struct SelectResult {
    pub remote: Remote,
    pub repo: Rc<Repo>,
    pub exists: bool,
}

impl SelectResult {
    pub fn as_repo(self) -> (Remote, Rc<Repo>) {
        (self.remote, self.repo)
    }
}

pub struct Query<'a> {
    db: &'a Database,

    query: Vec<String>,
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
        Query { db, query }
    }

    pub fn select(&self, opts: SelectOptions) -> Result<SelectResult> {
        if self.query.is_empty() {
            let repo = if opts.search {
                self.search_local(self.db.list_all(), NameLevel::Full)?
            } else {
                self.db.must_latest("")?
            };
            let remote = config::must_get_remote(repo.remote.as_str())?;
            return Ok(SelectResult {
                remote,
                repo,
                exists: true,
            });
        }

        if self.query.len() == 1 {
            let maybe_remote = &self.query[0];
            return match config::get_remote(maybe_remote)? {
                Some(remote) => {
                    let repo = if opts.search {
                        self.search_local(self.db.list_by_remote(&remote.name), NameLevel::Owner)?
                    } else {
                        self.db.must_latest(&remote.name)?
                    };
                    let remote = config::must_get_remote(repo.remote.as_str())?;
                    Ok(SelectResult {
                        remote,
                        repo,
                        exists: true,
                    })
                }
                None => {
                    let repo = self.db.must_get_fuzzy("", maybe_remote)?;
                    let remote = config::must_get_remote(repo.remote.as_str())?;
                    Ok(SelectResult {
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

            let mut search_local = opts.local_only;
            if let None = remote.provider {
                search_local = true;
            }
            if search_local {
                let repo = self.search_local(
                    self.db.list_by_owner(remote.name.as_str(), owner),
                    NameLevel::Name,
                )?;
                return Ok(SelectResult {
                    remote,
                    repo,
                    exists: true,
                });
            }

            let provider = api::init_provider(&remote, opts.force)?;
            let api_repos = provider.list_repos(owner)?;
            let idx = shell::search(&api_repos)?;
            let name = &api_repos[idx];
            return Ok(self.get_repo(remote, owner, name.as_str(), &opts));
        }

        let (owner, name) = parse_owner(query);
        if owner.is_empty() {
            let repo = self.db.must_get_fuzzy(&remote.name, &name)?;
            return Ok(SelectResult {
                remote,
                repo,
                exists: true,
            });
        }
        return Ok(self.get_repo(remote, owner, name, &opts));
    }

    pub fn must_select(&self, opts: SelectOptions) -> Result<(Remote, Rc<Repo>)> {
        let result = self.select(opts)?;
        if !result.exists {
            bail!("Could not find repo {}", result.repo.full_name());
        }
        Ok(result.as_repo())
    }

    pub fn select_remote(&self, opts: SelectOptions) -> Result<SelectResult> {
        assert!(self.query.len() == 2);
        let remote_name = &self.query[0];
        let remote = config::must_get_remote(remote_name)?;
        let query = &self.query[1];

        if query.ends_with("/") {
            let owner = query.strip_suffix("/").unwrap();
            let provider = api::init_provider(&remote, opts.force)?;
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
            return Ok(self.get_repo(remote, owner, name.as_str(), &opts));
        }

        let (owner, name) = parse_owner(query);
        if owner.is_empty() {
            bail!("Invalid args, the repo name should contain owner");
        }
        return Ok(self.get_repo(remote, owner, name, &opts));
    }

    fn search_local(&self, vec: Vec<Rc<Repo>>, level: NameLevel) -> Result<Rc<Repo>> {
        let items: Vec<_> = vec.iter().map(|repo| repo.as_string(&level)).collect();
        let idx = shell::search(&items)?;
        let repo = Rc::clone(&vec[idx]);
        Ok(repo)
    }

    fn get_repo<S>(&self, remote: Remote, owner: S, name: S, opts: &SelectOptions) -> SelectResult
    where
        S: AsRef<str>,
    {
        match self
            .db
            .get(remote.name.as_str(), owner.as_ref(), name.as_ref())
        {
            Some(repo) => SelectResult {
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
                    opts.repo_path.clone(),
                );
                SelectResult {
                    remote,
                    repo,
                    exists: false,
                }
            }
        }
    }

    pub fn list_local(&self, filter: bool) -> Result<(Vec<Rc<Repo>>, NameLevel)> {
        let (repos, level) = self.list_local_raw()?;
        if filter {
            let items: Vec<String> = repos.iter().map(|repo| repo.as_string(&level)).collect();
            let items = utils::edit_items(items)?;
            let items_set: HashSet<String> = items.into_iter().collect();

            let repos: Vec<Rc<Repo>> = repos
                .into_iter()
                .filter(|repo| items_set.contains(&repo.as_string(&level)))
                .collect();

            return Ok((repos, level));
        }
        Ok((repos, level))
    }

    fn list_local_raw(&self) -> Result<(Vec<Rc<Repo>>, NameLevel)> {
        if self.query.is_empty() {
            return Ok((self.db.list_all(), NameLevel::Full));
        }

        if self.query.len() == 1 {
            let maybe_remote = &self.query[0];
            return match config::get_remote(maybe_remote)? {
                Some(remote) => {
                    let repos = self.db.list_by_remote(&remote.name);
                    Ok((repos, NameLevel::Owner))
                }
                None => {
                    let repo = self.db.must_get_fuzzy("", maybe_remote)?;
                    Ok((vec![repo], NameLevel::Owner))
                }
            };
        }

        let remote_name = &self.query[0];
        let remote = config::must_get_remote(remote_name)?;
        let query = &self.query[1];

        if query.ends_with("/") {
            let owner = query.strip_suffix("/").unwrap();
            return Ok((
                self.db.list_by_owner(remote.name.as_str(), owner),
                NameLevel::Name,
            ));
        }

        let (owner, name) = parse_owner(&query);
        let repo = if owner.is_empty() {
            self.db.must_get_fuzzy(&remote.name, &name)?
        } else {
            self.db.must_get(&remote.name, &owner, &name)?
        };

        Ok((vec![repo], NameLevel::Name))
    }

    pub fn list_remote(&self, force: bool, filter: bool) -> Result<(Remote, String, Vec<String>)> {
        let (remote, owner, names) = self.list_remote_raw(force)?;
        if filter {
            let names = utils::edit_items(names.clone())?;
            return Ok((remote, owner, names));
        }

        Ok((remote, owner, names))
    }

    fn list_remote_raw(&self, force: bool) -> Result<(Remote, String, Vec<String>)> {
        assert!(self.query.len() == 2);

        let remote_name = &self.query[0];
        let remote = config::must_get_remote(remote_name)?;
        let query = &self.query[1];

        let (owner, name) = parse_owner(&query);
        if !owner.is_empty() && !name.is_empty() {
            return Ok((remote, owner, vec![query.clone()]));
        }
        let owner = match query.strip_suffix("/") {
            Some(owner) => owner,
            None => query.as_str(),
        };
        let provider = api::init_provider(&remote, force)?;
        let items = provider.list_repos(owner)?;

        let attached = self.db.list_by_owner(remote.name.as_str(), owner);
        let attached_set: HashSet<&str> = attached.iter().map(|repo| repo.name.as_str()).collect();

        let names: Vec<_> = items
            .into_iter()
            .filter(|name| !attached_set.contains(name.as_str()))
            .collect();
        Ok((remote, owner.to_string(), names))
    }
}

pub fn parse_owner(query: impl AsRef<str>) -> (String, String) {
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

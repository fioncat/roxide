use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::Rc;

use anyhow::{bail, Result};

use crate::config;
use crate::repo::bytes::Bytes;
use crate::repo::types::Repo;
use crate::utils::Lock;

pub struct Database {
    repos: Vec<Rc<Repo>>,
    path: PathBuf,
    lock: Lock,
}

impl Database {
    pub fn read() -> Result<Database> {
        let lock = Lock::acquire("database")?;
        let path = PathBuf::from(&config::base().metadir).join("database");
        let bytes = Bytes::read(&path)?;
        let repos: Vec<Rc<Repo>> = bytes.into();

        Ok(Database { repos, path, lock })
    }

    pub fn get<S>(&self, remote: S, owner: S, name: S) -> Option<Rc<Repo>>
    where
        S: AsRef<str>,
    {
        self.repos.iter().find_map(|repo| {
            if repo.remote.as_str().ne(remote.as_ref()) {
                return None;
            }
            if repo.owner.as_str().ne(owner.as_ref()) {
                return None;
            }
            if repo.name.as_str().eq(name.as_ref()) {
                return Some(Rc::clone(repo));
            }
            None
        })
    }

    pub fn must_get<S>(&self, remote: S, owner: S, name: S) -> Result<Rc<Repo>>
    where
        S: AsRef<str>,
    {
        match self.get(remote.as_ref(), owner.as_ref(), name.as_ref()) {
            Some(repo) => Ok(repo),
            None => bail!("Could not find repo {}/{}", owner.as_ref(), name.as_ref()),
        }
    }

    pub fn get_fuzzy<S>(&self, remote: S, query: S) -> Option<Rc<Repo>>
    where
        S: AsRef<str>,
    {
        self.repos.iter().find_map(|repo| {
            if !remote.as_ref().is_empty() && remote.as_ref().ne(repo.remote.as_str()) {
                return None;
            }
            if repo.name.as_str().contains(query.as_ref()) {
                return Some(Rc::clone(repo));
            }
            None
        })
    }

    pub fn must_get_fuzzy<S>(&self, remote: S, query: S) -> Result<Rc<Repo>>
    where
        S: AsRef<str>,
    {
        match self.get_fuzzy(remote.as_ref(), query.as_ref()) {
            Some(repo) => Ok(repo),
            None => bail!("Could not find matched repo with query {}", query.as_ref()),
        }
    }

    pub fn latest<S>(&self, remote: S) -> Option<Rc<Repo>>
    where
        S: AsRef<str>,
    {
        let dir = config::current_dir();
        let mut max_time = 0;
        let mut pos = None;
        for (idx, repo) in self.repos.iter().enumerate() {
            if repo.get_path().eq(dir) {
                continue;
            }
            if !remote.as_ref().is_empty() && repo.remote.as_str().ne(remote.as_ref()) {
                continue;
            }
            if repo.last_accessed > max_time {
                pos = Some(idx);
                max_time = repo.last_accessed;
            }
        }
        match pos {
            Some(idx) => Some(Rc::clone(&self.repos[idx])),
            None => None,
        }
    }

    pub fn must_latest<S>(&self, remote: S) -> Result<Rc<Repo>>
    where
        S: AsRef<str>,
    {
        match self.latest(remote) {
            Some(repo) => Ok(repo),
            None => bail!("Could not find latest repo"),
        }
    }

    pub fn current(&self) -> Option<Rc<Repo>> {
        let dir = config::current_dir();
        self.repos.iter().find_map(|repo| {
            if repo.get_path().eq(dir) {
                return Some(Rc::clone(repo));
            }
            None
        })
    }

    pub fn must_current(&self) -> Result<Rc<Repo>> {
        match self.current() {
            Some(repo) => Ok(repo),
            None => bail!("You are not in a roxide repository"),
        }
    }

    pub fn list_all(&self) -> Vec<Rc<Repo>> {
        let mut list = Vec::with_capacity(self.repos.len());
        for repo in self.repos.iter() {
            list.push(Rc::clone(repo));
        }
        list
    }

    pub fn list_by_remote<S>(&self, remote: S) -> Vec<Rc<Repo>>
    where
        S: AsRef<str>,
    {
        let mut list = Vec::new();
        for repo in self.repos.iter() {
            if repo.remote.as_str().eq(remote.as_ref()) {
                list.push(Rc::clone(repo));
            }
        }
        list
    }

    pub fn list_by_owner<S>(&self, remote: S, owner: S) -> Vec<Rc<Repo>>
    where
        S: AsRef<str>,
    {
        let mut list = Vec::new();
        for repo in self.repos.iter() {
            if repo.remote.as_str().eq(remote.as_ref()) && repo.owner.as_str().eq(owner.as_ref()) {
                list.push(Rc::clone(repo));
            }
        }
        list
    }

    pub fn list_owners<S>(&self, remote: S) -> Vec<Rc<String>>
    where
        S: AsRef<str>,
    {
        let mut owners = Vec::new();
        let mut owner_set = HashSet::new();
        for repo in self.repos.iter() {
            if repo.remote.as_str().ne(remote.as_ref()) {
                continue;
            }
            if let None = owner_set.get(repo.owner.as_str()) {
                owner_set.insert(repo.owner.as_str());
                owners.push(Rc::clone(&repo.owner));
            }
        }
        owners
    }

    pub fn add(&mut self, repo: Rc<Repo>) {
        if let Some(idx) = self.position(&repo) {
            self.repos[idx] = repo;
        } else {
            self.repos.push(repo);
        }
    }

    pub fn update(&mut self, repo: Rc<Repo>) {
        self.add(repo.update());
    }

    pub fn remove(&mut self, repo: Rc<Repo>) {
        if let Some(idx) = self.position(&repo) {
            self.repos.remove(idx);
        }
    }

    pub fn close(self) -> Result<()> {
        let Database { repos, path, lock } = self;
        let bytes: Bytes = repos.into();
        bytes.save(&path)?;
        // Drop lock to release file lock after write done.
        drop(lock);
        Ok(())
    }

    fn position(&self, repo: &Rc<Repo>) -> Option<usize> {
        let mut pos = None;
        for (idx, to_update) in self.repos.iter().enumerate() {
            if to_update.eq(repo) {
                pos = Some(idx);
                break;
            }
        }
        pos
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    pub fn list_repos() -> Vec<Rc<Repo>> {
        vec![
            Repo::new("github", "fioncat", "roxide", None),
            Repo::new(
                "github",
                "fioncat",
                "spacenvim",
                Some(String::from("/path/to/nvim-config")),
            ),
            Repo::new("github", "fioncat", "csync", None),
            Repo::new("github", "fioncat", "fioncat", None),
            Repo::new("github", "fioncat", "dotfiles", None),
            Repo::new("github", "kubernetes", "kubernetes", None),
            Repo::new("github", "kubernetes", "kube-proxy", None),
            Repo::new("github", "kubernetes", "kubelet", None),
            Repo::new("github", "kubernetes", "kubectl", None),
            Repo::new("gitlab", "my-owner-01", "my-repo-01", None),
            Repo::new("gitlab", "my-owner-01", "my-repo-02", None),
            Repo::new("gitlab", "my-owner-01", "my-repo-03", None),
            Repo::new("gitlab", "my-owner-02", "my-repo-01", None),
            Repo::new("test", "rust", "hello", None),
            Repo::new("test", "rust", "test-roxide", None),
            Repo::new("test", "go", "hello", None),
            Repo::new("test", "python", "hello", None),
        ]
    }

    #[test]
    fn test_database() {
        let path = PathBuf::new();
        let repos = list_repos();
        let lock = Lock::acquire("test-database").unwrap();
        let db = Database { repos, path, lock };

        assert_eq!(
            db.get("github", "fioncat", "dotfiles"),
            Some(Repo::new("github", "fioncat", "dotfiles", None))
        );
        assert_eq!(
            db.get("github", "fioncat", "spacenvim"),
            Some(Repo::new(
                "github",
                "fioncat",
                "spacenvim",
                Some(String::from("/path/to/nvim-config"))
            ))
        );
        assert_eq!(
            db.get("test", "python", "hello"),
            Some(Repo::new("test", "python", "hello", None))
        );
        assert_eq!(db.get("test", "python", "unknown"), None);
        assert_eq!(db.get("github", "fioncat", "unknown"), None);
        assert_eq!(db.get("", "", ""), None);
        assert_eq!(
            db.get_fuzzy("github", "dot"),
            Some(Repo::new("github", "fioncat", "dotfiles", None))
        );
        assert_eq!(
            db.get_fuzzy("", "kube"),
            Some(Repo::new("github", "kubernetes", "kubernetes", None))
        );
        assert_eq!(
            db.get_fuzzy("", "ro"),
            Some(Repo::new("github", "fioncat", "roxide", None))
        );
        assert_eq!(
            db.get_fuzzy("gitlab", "my"),
            Some(Repo::new("gitlab", "my-owner-01", "my-repo-01", None))
        );
        assert_eq!(db.get_fuzzy("gitlab", "ro"), None);
        assert_eq!(db.get_fuzzy("", "zzzzzzzzzzzzzzz"), None);
    }
}

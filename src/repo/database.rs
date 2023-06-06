#![allow(dead_code)]

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::rc::Rc;

use anyhow::{bail, Context, Result};
use bincode::Options;
use serde::{Deserialize, Serialize};

use crate::repo::types::Repo;
use crate::{config, utils};

pub struct Database {
    repos: Vec<Rc<Repo>>,
    path: PathBuf,
}

impl Database {
    pub fn read() -> Result<Database> {
        let path = PathBuf::from(&config::base().metadir).join("database");
        let index = RepoIndex::read(&path)?;
        let repos: Vec<Rc<Repo>> = index.into();

        Ok(Database { repos, path })
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
        let max_time = 0;
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

    pub fn update(&mut self, repo: Rc<Repo>) {
        if let Some(idx) = self.position(&repo) {
            self.repos[idx] = repo.update();
        } else {
            self.repos.push(repo.update());
        }
    }

    pub fn remove(&mut self, repo: Rc<Repo>) {
        if let Some(idx) = self.position(&repo) {
            self.repos.remove(idx);
        }
    }

    pub fn close(self) -> Result<()> {
        let Database { repos, path } = self;
        let index: RepoIndex = repos.into();
        index.save(&path)
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

#[derive(Debug, Deserialize, Serialize)]
struct RepoInfo {
    pub path: Option<String>,
    pub last_accessed: u64,
    pub accessed: f64,
}

#[derive(Debug, Deserialize, Serialize)]
struct RepoIndex(HashMap<String, HashMap<String, HashMap<String, RepoInfo>>>);

impl RepoIndex {
    // Assume a maximum size for the database. This prevents bincode from
    // throwing strange errors when it encounters invalid data.
    const MAX_SIZE: u64 = 32 << 20;

    // Use Version to ensure that decode and encode are consistent.
    const VERSION: u32 = 1;

    fn read(path: &PathBuf) -> Result<RepoIndex> {
        match fs::read(path) {
            Ok(data) => Self::decode(&data),
            Err(err) if err.kind() == ErrorKind::NotFound => Ok(RepoIndex(HashMap::new())),
            Err(err) => Err(err).with_context(|| format!("Read index file {}", path.display())),
        }
    }

    fn decode(data: &[u8]) -> Result<RepoIndex> {
        let decoder = &mut bincode::options()
            .with_fixint_encoding()
            .with_limit(Self::MAX_SIZE);

        let version_size = decoder.serialized_size(&Self::VERSION).unwrap() as usize;
        if data.len() < version_size {
            bail!("corrupted data");
        }

        let (version_data, data) = data.split_at(version_size);
        let version: u32 = decoder
            .deserialize(version_data)
            .context("Decode version")?;
        if version != Self::VERSION {
            bail!("unsupported version {version}");
        }

        Ok(decoder.deserialize(data).context("Decode repo data")?)
    }

    fn save(&self, path: &PathBuf) -> Result<()> {
        let data = self.encode()?;
        utils::write_file(path, &data)
    }

    fn encode(&self) -> Result<Vec<u8>> {
        let buffer_size =
            bincode::serialized_size(&Self::VERSION)? + bincode::serialized_size(self)?;
        let mut buffer = Vec::with_capacity(buffer_size as usize);

        bincode::serialize_into(&mut buffer, &Self::VERSION).context("Encode version")?;
        bincode::serialize_into(&mut buffer, self).context("Encode repos")?;

        Ok(buffer)
    }
}

impl From<RepoIndex> for Vec<Rc<Repo>> {
    fn from(index: RepoIndex) -> Self {
        let mut repos = Vec::with_capacity(index.0.len());
        for (remote, owners) in index.0.into_iter() {
            let remote = Rc::new(remote);
            for (owner, repo_map) in owners.into_iter() {
                let owner = Rc::new(owner);
                for (repo_name, repo_info) in repo_map.into_iter() {
                    let name = Rc::new(repo_name);
                    let repo = Repo {
                        remote: Rc::clone(&remote),
                        owner: Rc::clone(&owner),
                        name: Rc::clone(&name),
                        accessed: repo_info.accessed,
                        last_accessed: repo_info.last_accessed,
                        path: repo_info.path,
                    };
                    repos.push(Rc::new(repo));
                }
            }
        }
        repos.sort_unstable_by(|repo1, repo2| repo2.score().total_cmp(&repo1.score()));

        repos
    }
}

impl From<Vec<Rc<Repo>>> for RepoIndex {
    fn from(repos: Vec<Rc<Repo>>) -> Self {
        let mut index_rc: HashMap<Rc<String>, HashMap<Rc<String>, HashMap<Rc<String>, RepoInfo>>> =
            HashMap::new();
        for repo in repos.into_iter() {
            let mut owners = match index_rc.remove(&repo.remote) {
                Some(owners) => owners,
                None => HashMap::new(),
            };

            let mut repo_map = match owners.remove(&repo.owner) {
                Some(repo_map) => repo_map,
                None => HashMap::new(),
            };

            let repo = Rc::try_unwrap(repo).expect("Unwrap repo RC failed");
            let Repo {
                remote,
                owner,
                name,
                path,
                last_accessed,
                accessed,
            } = repo;
            let info = RepoInfo {
                path,
                last_accessed,
                accessed,
            };
            repo_map.insert(name, info);
            owners.insert(owner, repo_map);
            index_rc.insert(remote, owners);
        }

        let mut index: HashMap<String, HashMap<String, HashMap<String, RepoInfo>>> =
            HashMap::with_capacity(index_rc.len());
        for (remote, owners_rc) in index_rc.into_iter() {
            let remote = Rc::try_unwrap(remote).expect("Failed to unwrap remote");
            let mut owners: HashMap<String, HashMap<String, RepoInfo>> =
                HashMap::with_capacity(owners_rc.len());

            for (owner, repo_map_rc) in owners_rc {
                let owner = Rc::try_unwrap(owner).expect("Failed to unwrap owner");
                let mut repo_map: HashMap<String, RepoInfo> =
                    HashMap::with_capacity(repo_map_rc.len());

                for (repo_name, repo_info) in repo_map_rc {
                    let name = Rc::try_unwrap(repo_name).expect("Failed to unwrap repo name");
                    repo_map.insert(name, repo_info);
                }

                owners.insert(owner, repo_map);
            }

            index.insert(remote, owners);
        }

        RepoIndex(index)
    }
}

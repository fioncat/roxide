use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use anyhow::{bail, Result};

use crate::config::Config;
use crate::repo::Repo;

pub fn get_path<S, R, O, N>(cfg: &Config, path: &Option<S>, remote: R, owner: O, name: N) -> PathBuf
where
    R: AsRef<str>,
    O: AsRef<str>,
    N: AsRef<str>,
    S: AsRef<str>,
{
    if let Some(path) = path.as_ref() {
        return PathBuf::from(path.as_ref());
    }

    cfg.get_workspace_dir()
        .join(remote.as_ref())
        .join(owner.as_ref())
        .join(name.as_ref())
}

struct RemoteBucket(HashMap<String, OwnerBucket>);

struct OwnerBucket(HashMap<String, RepoBucket>);

struct RepoBucket {
    path: Option<String>,
    labels: Option<HashSet<u64>>,

    last_accessed: u64,
    accessed: f64,
}

struct Bucket {
    data: HashMap<String, RemoteBucket>,

    label_index: u64,
    labels: HashMap<u64, String>,
}

pub struct Database<'a> {
    cfg: &'a Config,

    bucket: Bucket,

    path: PathBuf,

    clean_labels: bool,
}

impl Database<'_> {
    pub fn get<R, O, N>(&self, remote: R, owner: O, name: N) -> Option<Repo>
    where
        R: AsRef<str>,
        O: AsRef<str>,
        N: AsRef<str>,
    {
        let (remote, remote_bucket) = self.bucket.data.get_key_value(remote.as_ref())?;
        let (owner, owner_bucket) = remote_bucket.0.get_key_value(owner.as_ref())?;
        let (name, repo_bucket) = owner_bucket.0.get_key_value(name.as_ref())?;

        Some(self.build_repo(remote, owner, name, repo_bucket))
    }

    /// Similar to [`Database::get`], but returns error if the repository is not found.
    pub fn must_get<R, O, N>(&self, remote: R, owner: O, name: N) -> Result<Repo>
    where
        R: AsRef<str>,
        O: AsRef<str>,
        N: AsRef<str>,
    {
        match self.get(remote.as_ref(), owner.as_ref(), name.as_ref()) {
            Some(repo) => Ok(repo),
            None => bail!(
                "repo '{}:{}/{}' not found",
                remote.as_ref(),
                owner.as_ref(),
                name.as_ref()
            ),
        }
    }

    /// Locate a repository using a keyword. As long as the repository name contains
    /// the keyword, it is considered a successful match. The function prioritizes
    /// matches based on the repository's score.
    pub fn get_fuzzy<R, K>(&self, remote: R, keyword: K) -> Option<Repo>
    where
        R: AsRef<str>,
        K: AsRef<str>,
    {
        let repos = self.scan(remote, "", |_remote, _owner, name, _bucket| {
            Some(name.contains(keyword.as_ref()))
        })?;
        self.get_max_score(repos)
    }

    /// Similar to [`Database::get_fuzzy`], but returns error if the repository
    /// is not found.
    pub fn must_get_fuzzy<R, K>(&self, remote: R, keyword: K) -> Result<Repo>
    where
        R: AsRef<str>,
        K: AsRef<str>,
    {
        match self.get_fuzzy(remote.as_ref(), keyword.as_ref()) {
            Some(repo) => Ok(repo),
            None => {
                if remote.as_ref().is_empty() {
                    bail!(
                        "cannot find repo that contains keyword '{}'",
                        keyword.as_ref()
                    );
                }
                bail!(
                    "cannot find repo that contains keyword '{}' in remote '{}'",
                    keyword.as_ref(),
                    remote.as_ref()
                );
            }
        }
    }

    /// Retrieve the most recently accessed repository. This function follows
    /// certain rules:
    ///
    /// 1. If the current position is exactly at the most recently accessed
    /// repository, the function will not return the current repository.
    /// 2. If the current position is within a subdirectory of a repository,
    /// it will directly return the current repository.
    ///
    /// This design is intended to make the function more flexible, allowing it to
    /// achieve effects similar to `cd -` in Linux.
    pub fn get_latest(&self, remote: impl AsRef<str>) -> Option<Repo> {
        let repos = self.scan(remote, "", |remote, owner, name, bucket| {
            let repo_path = get_path(self.cfg, &bucket.path, remote, owner, name);
            if repo_path.eq(self.cfg.get_current_dir()) {
                // If we are currently in the root directory of a repo, the latest
                // method should return another repo, so the current repo will be
                // skipped here.
                return Some(false);
            }

            if self.cfg.get_current_dir().starts_with(&repo_path) {
                // If we are currently in a subdirectory of a repo, latest will
                // directly return this repo.
                return None;
            }

            Some(true)
        })?;
        repos.into_iter().max_by_key(|repo| repo.last_accessed)
    }

    /// Similar to `get_latest`, but returns error if the repository is not found.
    pub fn must_get_latest<S>(&self, remote: S) -> Result<Repo>
    where
        S: AsRef<str>,
    {
        match self.get_latest(remote.as_ref()) {
            Some(repo) => Ok(repo),
            None => {
                if remote.as_ref().is_empty() {
                    bail!("no latest repo");
                }
                bail!("no latest repo in remote '{}'", remote.as_ref());
            }
        }
    }

    /// Return the repository currently being accessed.
    pub fn get_current(&self) -> Option<Repo> {
        let repos = self.scan("", "", |remote, owner, name, bucket| {
            let repo_path = get_path(self.cfg, &bucket.path, remote, owner, name);
            if repo_path.eq(self.cfg.get_current_dir()) {
                return None;
            }
            Some(false)
        })?;
        repos.into_iter().next()
    }

    /// Return the repository currently being accessed. If not within any
    /// repository, return an error.
    pub fn must_get_current(&self) -> Result<Repo> {
        match self.get_current() {
            Some(repo) => Ok(repo),
            None => bail!("you are not in a repo"),
        }
    }

    /// List all repositories.
    pub fn list_all(&self, labels: &Option<HashSet<String>>) -> Vec<Repo> {
        let mut repos = self
            .scan("", "", |_remote, _owner, _name, bucket| {
                self.filter_labels(bucket, labels)
            })
            .unwrap_or(Vec::new());
        self.sort_repos(&mut repos);
        repos
    }

    /// List all repositories under a remote.
    pub fn list_by_remote(
        &self,
        remote: impl AsRef<str>,
        labels: &Option<HashSet<String>>,
    ) -> Vec<Repo> {
        let mut repos = self
            .scan(remote, "", |_remote, _owner, _name, bucket| {
                self.filter_labels(bucket, labels)
            })
            .unwrap_or(Vec::new());
        self.sort_repos(&mut repos);
        repos
    }

    /// List all owner names.
    pub fn list_owners(&self, remote: impl AsRef<str>) -> Vec<String> {
        (|| {
            let remote_bucket = self.bucket.data.get(remote.as_ref())?;
            let mut owners: Vec<String> =
                remote_bucket.0.keys().map(|key| key.to_string()).collect();
            owners.sort_unstable();
            Some(owners)
        })()
        .unwrap_or(Vec::new())
    }

    pub fn upsert(&mut self, repo: Repo) {
        let (remote, mut remote_bucket) = self
            .bucket
            .data
            .remove_entry(repo.remote.as_ref())
            .unwrap_or((
                repo.remote.to_string(),
                RemoteBucket(HashMap::with_capacity(1)),
            ));

        let (owner, mut owner_bucket) = remote_bucket
            .0
            .remove_entry(repo.owner.as_ref())
            .unwrap_or((
                repo.owner.to_string(),
                OwnerBucket(HashMap::with_capacity(1)),
            ));

        let (name, mut repo_bucket) = owner_bucket.0.remove_entry(repo.name.as_ref()).unwrap_or((
            repo.name.to_string(),
            RepoBucket {
                path: None,
                labels: None,
                last_accessed: 0,
                accessed: 0.0,
            },
        ));

        let labels_index = self.build_labels_index(&repo);
        if labels_index != repo_bucket.labels {
            repo_bucket.labels = labels_index;
            self.clean_labels = true;
        }

        repo_bucket.path = repo.path.map(|path| path.to_string());
        repo_bucket.last_accessed = repo.last_accessed;
        repo_bucket.accessed = repo.accessed;

        owner_bucket.0.insert(name, repo_bucket);
        remote_bucket.0.insert(owner, owner_bucket);
        self.bucket.data.insert(remote, remote_bucket);
    }

    pub fn remove(&mut self, repo: Repo) {
        if let Some((remote, mut remote_bucket)) =
            self.bucket.data.remove_entry(repo.remote.as_ref())
        {
            if let Some((owner, mut owner_bucket)) =
                remote_bucket.0.remove_entry(repo.owner.as_ref())
            {
                if let Some(bucket) = owner_bucket.0.remove(repo.name.as_ref()) {
                    if let Some(_) = bucket.labels {
                        self.clean_labels = true;
                    }
                };
                if !owner_bucket.0.is_empty() {
                    remote_bucket.0.insert(owner, owner_bucket);
                }
            }

            if !remote_bucket.0.is_empty() {
                self.bucket.data.insert(remote, remote_bucket);
            }
        }
    }

    /// List all repositories under an owner.
    pub fn list_by_owner<R, O>(
        &self,
        remote: R,
        owner: O,
        labels: &Option<HashSet<String>>,
    ) -> Vec<Repo>
    where
        R: AsRef<str>,
        O: AsRef<str>,
    {
        let mut repos = self
            .scan(remote, owner, |_remote, _owner, _name, bucket| {
                self.filter_labels(bucket, labels)
            })
            .unwrap_or(Vec::new());
        self.sort_repos(&mut repos);
        repos
    }

    #[inline]
    fn filter_labels(&self, bucket: &RepoBucket, labels: &Option<HashSet<String>>) -> Option<bool> {
        if let None = labels {
            return Some(true);
        }
        let labels = labels.as_ref().unwrap();
        if labels.is_empty() {
            return Some(true);
        }

        let repo_labels = match self.build_labels(bucket) {
            Some(labels) => labels,
            None => return Some(false),
        };

        for label in labels {
            if !repo_labels.contains(label.as_str()) {
                return Some(false);
            }
        }
        Some(true)
    }

    #[inline]
    fn get_max_score<'a>(&'a self, repos: Vec<Repo<'a>>) -> Option<Repo<'a>> {
        repos.into_iter().max_by_key(|repo| repo.score(self.cfg))
    }

    #[inline]
    fn sort_repos(&self, repos: &mut Vec<Repo>) {
        repos.sort_unstable_by(|a, b| b.score(self.cfg).cmp(&a.score(self.cfg)))
    }

    #[inline]
    fn scan<R, O, F>(&self, remote: R, owner: O, filter: F) -> Option<Vec<Repo>>
    where
        R: AsRef<str>,
        O: AsRef<str>,
        F: Fn(&String, &String, &String, &RepoBucket) -> Option<bool>,
    {
        if remote.as_ref().is_empty() {
            let mut repos = Vec::new();
            for (remote, remote_bucket) in self.bucket.data.iter() {
                repos.extend(self.scan_remote(remote, remote_bucket, &filter));
            }
            return Some(repos);
        }
        let (remote, remote_bucket) = self.bucket.data.get_key_value(remote.as_ref())?;
        if owner.as_ref().is_empty() {
            return Some(self.scan_remote(remote, remote_bucket, &filter));
        }

        let (owner, owner_bucket) = remote_bucket.0.get_key_value(owner.as_ref())?;
        Some(self.scan_owner(remote, owner, owner_bucket, &filter))
    }

    #[inline]
    fn scan_remote<'a, F>(
        &'a self,
        remote: &'a String,
        remote_bucket: &'a RemoteBucket,
        filter: &F,
    ) -> Vec<Repo<'a>>
    where
        F: Fn(&String, &String, &String, &RepoBucket) -> Option<bool>,
    {
        let mut repos = Vec::new();
        for (owner, owner_bucket) in remote_bucket.0.iter() {
            repos.extend(self.scan_owner(remote, owner, owner_bucket, filter));
        }
        repos
    }

    #[inline]
    fn scan_owner<'a, F>(
        &'a self,
        remote: &'a String,
        owner: &'a String,
        owner_bucket: &'a OwnerBucket,
        filter: &F,
    ) -> Vec<Repo<'a>>
    where
        F: Fn(&String, &String, &String, &RepoBucket) -> Option<bool>,
    {
        let mut repos = Vec::new();
        for (name, repo_bucket) in owner_bucket.0.iter() {
            match filter(remote, owner, name, repo_bucket) {
                Some(ok) => {
                    if !ok {
                        continue;
                    }
                }
                None => return vec![self.build_repo(remote, owner, name, repo_bucket)],
            }
            let repo = self.build_repo(remote, owner, name, repo_bucket);
            repos.push(repo);
        }
        repos
    }

    #[inline]
    fn build_repo<'a>(
        &'a self,
        remote: &'a String,
        owner: &'a String,
        name: &'a String,
        bucket: &'a RepoBucket,
    ) -> Repo<'a> {
        Repo {
            remote: Cow::Borrowed(remote),
            owner: Cow::Borrowed(owner),
            name: Cow::Borrowed(name),

            path: bucket
                .path
                .as_ref()
                .map(|path| Cow::Borrowed(path.as_str())),

            labels: self.build_labels(bucket),

            last_accessed: bucket.last_accessed,
            accessed: bucket.accessed,

            remote_cfg: self.cfg.get_remote(remote),
            owner_cfg: self.cfg.get_owner(remote, owner),
        }
    }

    #[inline]
    fn build_labels<'a>(&'a self, bucket: &'a RepoBucket) -> Option<HashSet<Cow<'a, str>>> {
        let labels_index = bucket.labels.as_ref()?;
        let mut labels = HashSet::with_capacity(labels_index.len());
        for id in labels_index {
            let label = match self.bucket.labels.get(id) {
                Some(label) => label,
                None => continue,
            };
            labels.insert(Cow::Borrowed(label.as_str()));
        }
        Some(labels)
    }

    fn build_labels_index(&mut self, repo: &Repo) -> Option<HashSet<u64>> {
        let labels = repo.labels.as_ref()?;

        let label_name_map: HashMap<_, _> = self
            .bucket
            .labels
            .iter()
            .map(|(id, name)| (name.to_string(), *id))
            .collect();

        let mut index = HashSet::with_capacity(labels.len());
        for label in labels {
            let id = label_name_map
                .get(label.as_ref())
                .map(|id| *id)
                .unwrap_or_else(|| {
                    let id = self.bucket.label_index;
                    self.bucket.label_index += 1;
                    self.bucket.labels.insert(id, label.to_string());
                    id
                });
            index.insert(id);
        }

        Some(index)
    }
}

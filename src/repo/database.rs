#![allow(dead_code)]

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;
use std::{fs, io};

use anyhow::{bail, Context, Result};
use bincode::Options;
use reqwest::Url;
use serde::{Deserialize, Serialize};

use crate::api::Provider;
use crate::config::Config;
use crate::repo::{NameLevel, Owner, Remote, Repo};
use crate::utils::{self, FileLock};

/// The Bucket is repositories data structure stored on disk that can be directly
/// serialized and deserialized.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Bucket {
    remotes: Vec<String>,
    owners: Vec<String>,
    labels: Vec<String>,
    repos: Vec<RepoBucket>,
}

/// The RepoBucket is a repository data structure stored on disk that can be directly
/// serialized and deserialized.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct RepoBucket {
    remote: u32,
    owner: u32,
    name: String,
    path: Option<String>,
    last_accessed: u64,
    accessed: f64,
    labels: Option<HashSet<u32>>,
}

impl Bucket {
    /// Assume a maximum size for the database. This prevents bincode from
    /// throwing strange errors when it encounters invalid data.
    const MAX_SIZE: u64 = 32 << 20;

    /// Use Version to ensure that decode and encode are consistent.
    const VERSION: u32 = 1;

    /// Return empty Bucket, with no repository data.
    fn empty() -> Self {
        Bucket {
            remotes: Vec::new(),
            owners: Vec::new(),
            repos: Vec::new(),
            labels: Vec::new(),
        }
    }

    /// Read and decode database file.
    pub fn read(path: &PathBuf) -> Result<Bucket> {
        match fs::read(path) {
            Ok(data) => Self::decode(&data),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(Bucket::empty()),
            Err(err) => Err(err).with_context(|| format!("read database file {}", path.display())),
        }
    }

    /// Decode binary data to Bucket.
    fn decode(data: &[u8]) -> Result<Bucket> {
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
            .context("decode version")?;
        if version != Self::VERSION {
            bail!("unsupported version {version}");
        }

        Ok(decoder.deserialize(data).context("decode repo data")?)
    }

    /// Encode and write binary data to a file.
    pub fn save(&self, path: &PathBuf) -> Result<()> {
        let data = self.encode()?;
        utils::write_file(path, &data)
    }

    /// Encode bucket to binary data.
    fn encode(&self) -> Result<Vec<u8>> {
        let buffer_size =
            bincode::serialized_size(&Self::VERSION)? + bincode::serialized_size(self)?;
        let mut buffer = Vec::with_capacity(buffer_size as usize);

        bincode::serialize_into(&mut buffer, &Self::VERSION).context("Encode version")?;
        bincode::serialize_into(&mut buffer, self).context("Encode repos")?;

        Ok(buffer)
    }

    /// To save metadata storage space, transform the list of repositories
    /// suitable for queries into data suitable for storage in buckets.
    ///
    /// The bucket data structure undergoes some special optimizations—it, stores
    /// remote, owner, and label data only once, while repositories store indices
    /// of these data. Information about remote, owner, and label for a particular
    /// repository is located through these indices.
    ///
    /// # Panics
    ///
    /// Please ensure that the `Rc` references in `repos` are unique when calling
    /// this method; since we will try to unwrap them, otherwise, the function
    /// will panic.
    ///
    /// # Arguments
    ///
    /// * `repos` - The repositories to convert.
    fn from_repos(repos: Vec<Rc<Repo>>) -> Bucket {
        let mut remotes: Vec<String> = Vec::new();
        let mut remotes_index: HashMap<String, u32> = HashMap::new();

        let mut owners: Vec<String> = Vec::new();
        let mut owners_index: HashMap<String, u32> = HashMap::new();

        let mut all_labels: Vec<String> = Vec::new();
        let mut all_labels_index: HashMap<String, u32> = HashMap::new();

        let mut repo_buckets: Vec<RepoBucket> = Vec::with_capacity(repos.len());
        for repo in repos {
            let remote_idx = match remotes_index.get(repo.remote.name.as_str()) {
                Some(idx) => *idx,
                None => {
                    let remote_name = repo.remote.name.to_string();
                    let idx = remotes.len() as u32;
                    remotes_index.insert(remote_name.clone(), idx);
                    remotes.push(remote_name);
                    idx
                }
            };

            let owner_idx = match owners_index.get(repo.owner.name.as_str()) {
                Some(idx) => *idx,
                None => {
                    let owner_name = repo.owner.name.to_string();
                    let idx = owners.len() as u32;
                    owners_index.insert(owner_name.clone(), idx);
                    owners.push(owner_name);
                    idx
                }
            };

            let labels_idx = match repo.labels.as_ref() {
                Some(labels_set) => {
                    let mut labels_idx = HashSet::with_capacity(labels_set.len());
                    for label in labels_set.iter() {
                        let idx = match all_labels_index.get(label.as_str()) {
                            Some(idx) => *idx,
                            None => {
                                let label_name = label.to_string();
                                let idx = all_labels.len() as u32;
                                all_labels_index.insert(label_name.clone(), idx);
                                all_labels.push(label_name);
                                idx
                            }
                        };
                        labels_idx.insert(idx);
                    }
                    Some(labels_idx)
                }
                None => None,
            };

            // Here, directly unwrap the Rc; it is the responsibility of the
            // external code to guarantee that the current repository has a unique
            // reference.
            let repo = Rc::try_unwrap(repo).expect("Unwrap repo rc failed");
            let Repo {
                name,
                remote: _,
                owner: _,
                path,
                last_accessed,
                accessed,
                labels: _,
            } = repo;

            repo_buckets.push(RepoBucket {
                remote: remote_idx,
                owner: owner_idx,
                name,
                path,
                last_accessed,
                accessed,
                labels: labels_idx,
            })
        }

        Bucket {
            remotes,
            owners,
            labels: all_labels,
            repos: repo_buckets,
        }
    }

    /// Convert the bucket data suitable for storage into repositories suitable for
    /// queries; this function is the reverse process of `from_repos`.
    /// The constructed repositories will all be Rc pointers to save memory and
    /// clone costs. This also implies that they are entirely read-only; if modifications
    /// are necessary, new repository objects may need to be created.
    ///
    /// # Arguments
    ///
    /// * `cfg` - The config is used to inject remote and owner configuration
    /// information into the generated repository object for convenient subsequent
    /// calls.
    fn build_repos(self, cfg: &Config) -> Result<Vec<Rc<Repo>>> {
        let Bucket {
            remotes: remote_names,
            owners: owner_names,
            labels: all_label_names,
            repos: all_buckets,
        } = self;

        let mut remote_index: HashMap<u32, Rc<Remote>> = HashMap::with_capacity(remote_names.len());
        for (idx, remote_name) in remote_names.into_iter().enumerate() {
            let remote_cfg = match cfg.get_remote(&remote_name)  {
                Some(remote) => remote,
                None => bail!("could not find remote config for '{remote_name}', please add it to your config file"),
            };
            let remote = Rc::new(Remote {
                name: remote_name,
                cfg: remote_cfg.clone(),
            });

            remote_index.insert(idx as u32, remote);
        }

        let owner_name_index: HashMap<u32, String> = owner_names
            .into_iter()
            .enumerate()
            .map(|(idx, owner)| (idx as u32, owner))
            .collect();
        let mut owner_index: HashMap<String, HashMap<String, Rc<Owner>>> =
            HashMap::with_capacity(owner_name_index.len());

        let label_name_index: HashMap<u32, Rc<String>> = all_label_names
            .into_iter()
            .enumerate()
            .map(|(idx, label)| (idx as u32, Rc::new(label)))
            .collect();

        let mut repos: Vec<Rc<Repo>> = Vec::with_capacity(all_buckets.len());
        for repo_bucket in all_buckets {
            let remote = match remote_index.get(&repo_bucket.remote) {
                Some(remote) => Rc::clone(remote),
                // If the remote recorded in the repository does not exist in the
                // index, it indicates that this is dirty data (manually generated?
                // or there is a bug). Skip processing here.
                // FIXME: If this is caused by a bug, should we panic or report an
                // error here to expose the issue better?
                None => continue,
            };

            let owner_name = match owner_name_index.get(&repo_bucket.owner) {
                Some(owner_name) => owner_name,
                // The same as remote, may have bug
                None => continue,
            };

            // If the owner already exists in the cache, reuse it using Rc. If it
            // doesn't exist, construct and cache it using a combination of coordination
            // and indexing.
            let owner = match owner_index.get(&remote.name) {
                Some(m) => match m.get(owner_name) {
                    Some(owner) => Some(Rc::clone(owner)),
                    None => None,
                },
                None => None,
            };
            let owner = match owner {
                Some(o) => o,
                None => {
                    // Build the owner object
                    let owner_name = owner_name.clone();
                    let owner_remote = Rc::clone(&remote);
                    let owner = match cfg.get_owner(&remote.name, &owner_name) {
                        Some(owner_cfg) => Rc::new(Owner {
                            name: owner_name,
                            remote: owner_remote,
                            cfg: Some(owner_cfg.clone()),
                        }),
                        None => Rc::new(Owner {
                            name: owner_name,
                            remote: owner_remote,
                            cfg: None,
                        }),
                    };
                    match owner_index.get_mut(&remote.name) {
                        Some(m) => {
                            m.insert(owner.name.to_string(), Rc::clone(&owner));
                        }
                        None => {
                            let mut m = HashMap::with_capacity(1);
                            m.insert(owner.name.to_string(), Rc::clone(&owner));
                            owner_index.insert(remote.name.to_string(), m);
                        }
                    }
                    owner
                }
            };

            let labels = match repo_bucket.labels.as_ref() {
                Some(labels_index) => {
                    let mut labels = HashSet::with_capacity(labels_index.len());
                    for idx in labels_index {
                        if let Some(label_name) = label_name_index.get(idx) {
                            labels.insert(Rc::clone(label_name));
                        }
                    }

                    Some(labels)
                }
                None => None,
            };

            let repo = Rc::new(Repo {
                name: repo_bucket.name,
                remote,
                owner,
                path: repo_bucket.path,
                last_accessed: repo_bucket.last_accessed,
                accessed: repo_bucket.accessed,
                labels,
            });

            repos.push(repo);
        }
        repos.sort_unstable_by(|repo1, repo2| repo2.score(cfg).total_cmp(&repo1.score(cfg)));

        Ok(repos)
    }
}

/// The Database provides basic functions for querying the repository, enabling
/// location identification and listing. It also supports updating and deleting
/// repository data. However, changes made need to be written to disk by calling
/// the `save` function after the operations.
pub struct Database<'a> {
    repos: Vec<Rc<Repo>>,
    path: PathBuf,

    /// The Database can only be operated by one process at a time, so file lock
    /// here are used here to provide protection.
    lock: FileLock,

    cfg: &'a Config,
}

impl Database<'_> {
    /// Load the database.
    /// The database file is located at the path `{metadir}/database`.
    /// This function will attempt to acquire the file lock for the database file.
    /// If the database file does not exist, the load function will return an empty
    /// database, suitable for handling the initial condition.
    pub fn load(cfg: &Config) -> Result<Database> {
        let lock = FileLock::acquire(cfg, "database")?;
        let path = cfg.get_meta_dir().join("database");
        let bucket = Bucket::read(&path)?;
        let repos = bucket.build_repos(cfg)?;

        Ok(Database {
            repos,
            path,
            lock,
            cfg,
        })
    }

    /// Locate a repository based on its name.
    pub fn get<R, O, N>(&self, remote_name: R, owner_name: O, name: N) -> Option<Rc<Repo>>
    where
        R: AsRef<str>,
        O: AsRef<str>,
        N: AsRef<str>,
    {
        self.repos.iter().find_map(|repo| {
            if repo.remote.name.as_str() != remote_name.as_ref() {
                return None;
            }
            if repo.owner.name.as_str() != owner_name.as_ref() {
                return None;
            }
            if repo.name.as_str() == name.as_ref() {
                return Some(Rc::clone(repo));
            }
            None
        })
    }

    /// Similar to `get_repo`, but returns error if the repository is not found.
    pub fn must_get<R, O, N>(&self, remote_name: R, owner_name: O, name: N) -> Result<Rc<Repo>>
    where
        R: AsRef<str>,
        O: AsRef<str>,
        N: AsRef<str>,
    {
        match self.get(remote_name.as_ref(), owner_name.as_ref(), name.as_ref()) {
            Some(repo) => Ok(repo),
            None => bail!(
                "repo '{}:{}/{}' not found",
                remote_name.as_ref(),
                owner_name.as_ref(),
                name.as_ref()
            ),
        }
    }

    /// Locate a repository using a keyword. As long as the repository name contains
    /// the keyword, it is considered a successful match. The function prioritizes
    /// matches based on the repository's score.
    pub fn get_fuzzy<R, K>(&self, remote_name: R, keyword: K) -> Option<Rc<Repo>>
    where
        R: AsRef<str>,
        K: AsRef<str>,
    {
        self.repos.iter().find_map(|repo| {
            if !remote_name.as_ref().is_empty() && remote_name.as_ref() != repo.remote.name.as_str()
            {
                return None;
            }
            if repo.name.as_str().contains(keyword.as_ref()) {
                return Some(Rc::clone(repo));
            }
            None
        })
    }

    /// Similar to `get_fuzzy`, but returns error if the repository is not found.
    pub fn must_get_fuzzy<R, K>(&self, remote_name: R, keyword: K) -> Result<Rc<Repo>>
    where
        R: AsRef<str>,
        K: AsRef<str>,
    {
        match self.get_fuzzy(remote_name.as_ref(), keyword.as_ref()) {
            Some(repo) => Ok(repo),
            None => {
                if remote_name.as_ref().is_empty() {
                    bail!(
                        "cannot find repo that contains keyword '{}'",
                        keyword.as_ref()
                    );
                }
                bail!(
                    "cannot find repo that contains keyword '{}' in remote '{}'",
                    keyword.as_ref(),
                    remote_name.as_ref()
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
    /// This design is intended to make the `get_latest` function more flexible,
    /// allowing it to achieve effects similar to `cd -` in Linux.
    pub fn get_latest<S>(&self, remote_name: S) -> Option<Rc<Repo>>
    where
        S: AsRef<str>,
    {
        let mut max_time = 0;
        let mut pos = None;

        for (idx, repo) in self.repos.iter().enumerate() {
            let repo_path = repo.get_path(self.cfg);
            if repo_path.eq(self.cfg.get_current_dir()) {
                // If we are currently in the root directory of a repo, the latest
                // method should return another repo, so the current repo will be
                // skipped here.
                continue;
            }
            if self.cfg.get_current_dir().starts_with(&repo_path) {
                // If we are currently in a subdirectory of a repo, latest will
                // directly return this repo.
                if remote_name.as_ref().is_empty()
                    || repo.remote.name.as_str() == remote_name.as_ref()
                {
                    return Some(Rc::clone(repo));
                }
            }

            if !remote_name.as_ref().is_empty() && repo.remote.name.as_str() != remote_name.as_ref()
            {
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

    /// Similar to `get_latest`, but returns error if the repository is not found.
    pub fn must_get_latest<S>(&self, remote_name: S) -> Result<Rc<Repo>>
    where
        S: AsRef<str>,
    {
        match self.get_latest(remote_name.as_ref()) {
            Some(repo) => Ok(repo),
            None => {
                if remote_name.as_ref().is_empty() {
                    bail!("no latest repo");
                }
                bail!("no latest repo in remote '{}'", remote_name.as_ref());
            }
        }
    }

    /// Return the repository currently being accessed.
    pub fn get_current(&self) -> Option<Rc<Repo>> {
        self.repos.iter().find_map(|repo| {
            let repo_path = repo.get_path(self.cfg);
            if repo_path.eq(self.cfg.get_current_dir()) {
                return Some(Rc::clone(repo));
            }
            None
        })
    }

    /// Return the repository currently being accessed. If not within any
    /// repository, return an error.
    pub fn must_get_current(&self) -> Result<Rc<Repo>> {
        match self.get_current() {
            Some(repo) => Ok(repo),
            None => bail!("you are not in a repository"),
        }
    }

    /// List all repositories.
    pub fn list_all(&self, filter_labels: &Option<HashSet<String>>) -> Vec<Rc<Repo>> {
        let mut vec = Vec::with_capacity(self.repos.len());
        for repo in self.repos.iter() {
            if !repo.has_labels(filter_labels) {
                continue;
            }
            vec.push(Rc::clone(repo));
        }
        vec
    }

    /// List all repositories under a remote.
    pub fn list_by_remote<S>(
        &self,
        remote_name: S,
        filter_labels: &Option<HashSet<String>>,
    ) -> Vec<Rc<Repo>>
    where
        S: AsRef<str>,
    {
        let mut vec = Vec::with_capacity(self.repos.len());
        for repo in self.repos.iter() {
            if !repo.has_labels(filter_labels) {
                continue;
            }
            if repo.remote.name.as_str() == remote_name.as_ref() {
                vec.push(Rc::clone(repo));
            }
        }
        vec
    }

    /// List all repositories under an owner.
    pub fn list_by_owner<R, O>(
        &self,
        remote_name: R,
        owner_name: O,
        filter_labels: &Option<HashSet<String>>,
    ) -> Vec<Rc<Repo>>
    where
        R: AsRef<str>,
        O: AsRef<str>,
    {
        let mut vec = Vec::with_capacity(self.repos.len());
        for repo in self.repos.iter() {
            if !repo.has_labels(filter_labels) {
                continue;
            }
            if repo.remote.name.as_str() != remote_name.as_ref() {
                continue;
            }
            if repo.owner.name.as_str() != owner_name.as_ref() {
                continue;
            }

            vec.push(Rc::clone(repo));
        }
        vec
    }

    /// Update the repository data in the database. Call this after accessing a
    /// repository to update its access time and count. Additionally, you can
    /// update the repository's labels using the `labels` parameter.
    ///
    /// # Arguments
    ///
    /// * `repo` - The repository to update.
    /// * `labels` - If None, do not update labels, else, replace the labels with
    /// the given labels. If `labels` is not None but has no elements (where
    /// `is_empty` returns `true`), it will remove all the labels of the repository.
    pub fn update(&mut self, repo: Rc<Repo>, labels: Option<HashSet<String>>) {
        let pos = self.position(&repo);
        let new_repo = repo.update(self.cfg, labels);
        match pos {
            Some(idx) => self.repos[idx] = new_repo,
            None => self.repos.push(new_repo),
        }
    }

    /// Get the position of a repository.
    pub fn position(&self, repo: &Rc<Repo>) -> Option<usize> {
        let mut pos = None;
        for (idx, item) in self.repos.iter().enumerate() {
            if item.name.as_str() == repo.name.as_str()
                && item.remote.name.as_str() == repo.remote.name.as_str()
                && item.owner.name.as_str() == repo.owner.name.as_str()
            {
                pos = Some(idx);
                break;
            }
        }
        pos
    }

    /// Save the database changes to the disk file.
    pub fn save(self) -> Result<()> {
        let Database {
            repos,
            path,
            lock,
            cfg: _,
        } = self;
        let bucket = Bucket::from_repos(repos);
        bucket.save(&path)?;
        // Drop lock to release file lock after write done.
        drop(lock);
        Ok(())
    }
}

/// Used to provide some functionalities for the terminal. The main purpose of
/// defining this trait is for abstraction, especially for testing purposes where
/// it is inconvenient to directly use some functionalities of the terminal.
pub trait TerminalHelper {
    /// Searching in terminal, typically accomplished by directly invoking `fzf`.
    fn search(&self, items: &Vec<String>) -> Result<usize>;

    /// Use an editor to edit and filter multiple items.
    fn edit(&self, items: Vec<String>) -> Result<Vec<String>>;
}

/// Used to construct an API provider, this trait abstraction is primarily for
/// ease of testing.
pub trait ProviderBuilder {
    /// Build an API provider object.
    fn build_provider(
        &self,
        cfg: &Config,
        remote: &Remote,
        force: bool,
    ) -> Result<Box<dyn Provider>>;
}

/// In certain situations, the Selector needs to choose from multiple repositories.
/// This enum is used to define whether the preference during selection is towards
/// searching or fuzzy matching.
#[derive(Debug, Clone)]
pub enum SelectMode {
    Fuzzy,
    Search,
}

/// Define options for the Selector during the selection and filtering process.
#[derive(Debug, Clone)]
pub struct SelectOptions<T: TerminalHelper, P: ProviderBuilder> {
    terminal_helper: T,
    provider_builder: P,

    mode: SelectMode,

    force_no_cache: bool,

    force_local: bool,
    force_remote: bool,

    repo_path: Option<String>,

    many_edit: bool,

    filter_labels: Option<HashSet<String>>,
}

impl<T: TerminalHelper, P: ProviderBuilder> SelectOptions<T, P> {
    /// Create a new SelectOptions, with specified `TerminalHelper` and `ProviderBuilder`
    /// and default options. Useful for testing, in production, please use `default`.
    pub fn new(th: T, pb: P) -> SelectOptions<T, P> {
        SelectOptions {
            terminal_helper: th,
            provider_builder: pb,

            mode: SelectMode::Fuzzy,

            force_no_cache: false,

            force_local: false,
            force_remote: false,

            repo_path: None,

            many_edit: false,

            filter_labels: None,
        }
    }

    /// During selection, prioritize the use of searching over fuzzy matching.
    pub fn with_force_search(mut self, value: bool) -> Self {
        if value {
            self.mode = SelectMode::Search;
        }
        self
    }

    /// Control whether to use caching when building a provider.
    pub fn with_force_no_cache(mut self, value: bool) -> Self {
        self.force_no_cache = value;
        self
    }

    /// Control whether to search only in the local repository without relying on
    /// a provider.
    pub fn with_force_local(mut self, value: bool) -> Self {
        self.force_local = value;
        self
    }

    /// Control whether to use only the provider without searching in the local
    /// repository. If this option is enabled, searching will filter out all local
    /// repositories.
    pub fn with_force_remote(mut self, value: bool) -> Self {
        self.force_remote = value;
        self
    }

    /// Control whether to use a specified path instead of the workspace path when
    /// constructing a repository.
    pub fn with_repo_path(mut self, value: String) -> Self {
        self.repo_path = Some(value);
        self
    }

    /// Control the use of a terminal editor to filter results when selecting
    /// multiple repositories.
    pub fn with_many_edit(mut self, value: bool) -> Self {
        self.many_edit = value;
        self
    }

    /// Control the use of specified labels for filtering during search.
    pub fn with_filter_labels(mut self, labels: HashSet<String>) -> Self {
        self.filter_labels = Some(labels);
        self
    }

    /// Search repos from vec
    fn search_from_vec(&self, repos: &Vec<Rc<Repo>>, level: &NameLevel) -> Result<Rc<Repo>> {
        let items: Vec<String> = repos.iter().map(|repo| repo.to_string(&level)).collect();
        let idx = self.terminal_helper.search(&items)?;
        let result = match repos.get(idx) {
            Some(repo) => repo,
            None => bail!("internal error, terminal_helper returned an invalid index {idx}"),
        };

        Ok(Rc::clone(result))
    }
}

enum SelectOneType<'a> {
    Url(Url),
    Ssh(&'a str),
    Direct,
}

pub struct Selector<'a, T: TerminalHelper, P: ProviderBuilder> {
    head: &'a str,
    query: &'a str,

    db: &'a Database<'a>,

    opts: SelectOptions<T, P>,
}

impl<'a, T: TerminalHelper, P: ProviderBuilder> Selector<'_, T, P> {
    const GITHUB_DOMAIN: &'static str = "github.com";

    pub fn new(
        db: &'a Database<'a>,
        head: &'a str,
        query: &'a str,
        opts: SelectOptions<T, P>,
    ) -> Selector<'a, T, P> {
        Selector {
            head,
            query,
            db,
            opts,
        }
    }

    pub fn one(&self) -> Result<(Rc<Repo>, bool)> {
        if self.opts.force_remote {
            if self.head.is_empty() {
                bail!("internal error, when select one from provider, the head cannot be empty");
            }
            let remote_cfg = self.db.cfg.must_get_remote(self.head)?;
            let remote = Remote {
                name: self.head.to_string(),
                cfg: remote_cfg.clone(),
            };
            return self.one_from_provider(&remote);
        }

        if self.head.is_empty() {
            let repo = match self.opts.mode {
                SelectMode::Fuzzy => self.db.must_get_latest(""),
                SelectMode::Search => self.opts.search_from_vec(
                    &self.db.list_all(&self.opts.filter_labels),
                    &NameLevel::Remote,
                ),
            }?;
            return Ok((repo, true));
        }

        if self.query.is_empty() {
            let select_type = if self.head.ends_with(".git") {
                if self.head.starts_with("http") {
                    let url = self.head.strip_suffix(".git").unwrap();
                    SelectOneType::Url(
                        Url::parse(&url).with_context(|| format!("parse clone url '{url}'"))?,
                    )
                } else if self.head.starts_with("git@") {
                    SelectOneType::Ssh(self.head)
                } else {
                    match Url::parse(self.head) {
                        Ok(url) => SelectOneType::Url(url),
                        Err(_) => SelectOneType::Direct,
                    }
                }
            } else {
                match Url::parse(&self.head) {
                    Ok(url) => SelectOneType::Url(url),
                    Err(_) => SelectOneType::Direct,
                }
            };

            return match select_type {
                SelectOneType::Url(url) => self.one_from_url(url),
                SelectOneType::Ssh(ssh) => self.one_from_ssh(ssh),
                SelectOneType::Direct => self.one_from_head(),
            };
        }

        self.one_from_owner()
    }

    fn one_from_url(&self, url: Url) -> Result<(Rc<Repo>, bool)> {
        let domain = match url.domain() {
            Some(domain) => domain,
            None => bail!("missing domain in url '{url}'"),
        };
        let remote_names = self.db.cfg.list_remote_names();

        let mut target_remote_name: Option<String> = None;
        let mut is_gitlab = false;
        for name in remote_names {
            let remote_cfg = self.db.cfg.must_get_remote(&name)?;
            let remote_domain = match remote_cfg.clone.as_ref() {
                Some(domain) => domain,
                None => continue,
            };

            if remote_domain != domain {
                continue;
            }

            if remote_domain != Self::GITHUB_DOMAIN {
                is_gitlab = true;
            }
            target_remote_name = Some(name);
            break;
        }

        let remote_name = match target_remote_name {
            Some(remote) => remote,
            None => bail!("could not find remote whose clone domain is '{domain}' in url"),
        };

        let path_iter = match url.path_segments() {
            Some(iter) => iter,
            None => bail!("invalid url '{url}', path could not be empty"),
        };

        let mut parts = Vec::new();
        for part in path_iter {
            if is_gitlab {
                if part == "-" {
                    break;
                }
                parts.push(part);
                continue;
            }

            if parts.len() == 2 {
                break;
            }
            parts.push(part);
        }

        if parts.len() < 2 {
            bail!("invalid url '{url}', should be in a repo");
        }

        let path = parts.join("/");
        let (owner, name) = Self::parse_owner(&path);

        match self.db.get(&remote_name, &owner, &name) {
            Some(repo) => Ok((repo, true)),
            None => self.new_repo(&remote_name, &owner, &name),
        }
    }

    fn one_from_ssh(&self, ssh: &str) -> Result<(Rc<Repo>, bool)> {
        let full_name = ssh
            .strip_prefix("git@")
            .unwrap()
            .strip_suffix(".git")
            .unwrap();
        let full_name = full_name.replacen(":", "/", 1);

        let convert_url = format!("https://{full_name}");
        let url = Url::parse(&convert_url).with_context(|| {
            format!(
                "invalid ssh, parse url '{}' converted from ssh '{}'",
                convert_url, ssh
            )
        })?;

        self.one_from_url(url)
            .with_context(|| format!("select from ssh '{ssh}'"))
    }

    fn one_from_head(&self) -> Result<(Rc<Repo>, bool)> {
        match self.db.cfg.get_remote(self.head) {
            Some(_) => {
                let repo = match self.opts.mode {
                    SelectMode::Search => self.opts.search_from_vec(
                        &self.db.list_by_remote(self.head, &self.opts.filter_labels),
                        &NameLevel::Owner,
                    ),
                    SelectMode::Fuzzy => self.db.must_get_latest(self.head),
                }?;
                Ok((repo, true))
            }
            None => {
                let repo = self.db.must_get_fuzzy("", self.head)?;
                Ok((repo, true))
            }
        }
    }

    fn one_from_owner(&self) -> Result<(Rc<Repo>, bool)> {
        let remote_name = self.head;
        let remote_cfg = self.db.cfg.must_get_remote(remote_name)?;
        let remote = Remote {
            name: remote_name.to_string(),
            cfg: remote_cfg.clone(),
        };
        if self.query.ends_with("/") {
            let owner = self.query.strip_suffix("/").unwrap();
            let mut search_local = self.opts.force_local;
            if let None = remote_cfg.provider {
                search_local = true;
            }

            if search_local {
                let repo = self.opts.search_from_vec(
                    &self
                        .db
                        .list_by_owner(remote_name, owner, &self.opts.filter_labels),
                    &NameLevel::Name,
                )?;
                return Ok((repo, true));
            }

            let provider = self.opts.provider_builder.build_provider(
                &self.db.cfg,
                &remote,
                self.opts.force_no_cache,
            )?;
            let api_repos = provider.list_repos(owner)?;
            let idx = self.opts.terminal_helper.search(&api_repos)?;
            let name = &api_repos[idx];
            return self.get_or_create_repo(remote_name, owner, name);
        }

        let (owner, name) = Self::parse_owner(self.query);
        if owner.is_empty() {
            return match self.opts.mode {
                SelectMode::Search => self.one_from_provider(&remote),
                SelectMode::Fuzzy => {
                    let repo = self.db.must_get_fuzzy(remote_name, self.query)?;
                    Ok((repo, true))
                }
            };
        }

        self.get_or_create_repo(remote_name, &owner, &name)
    }

    fn one_from_provider(&self, remote: &Remote) -> Result<(Rc<Repo>, bool)> {
        let provider = self.opts.provider_builder.build_provider(
            &self.db.cfg,
            remote,
            self.opts.force_no_cache,
        )?;

        let items = provider.search_repos(self.query)?;
        if items.is_empty() {
            bail!("no result found from remote");
        }

        let idx = self.opts.terminal_helper.search(&items)?;
        let result = &items[idx];

        let (owner, name) = Self::parse_owner(result);
        if owner.is_empty() || name.is_empty() {
            bail!("invalid repo name '{result}' from remote");
        }

        self.get_or_create_repo(&remote.name, &owner, &name)
    }

    fn get_or_create_repo<S, O, N>(
        &self,
        remote_name: S,
        owner_name: O,
        name: N,
    ) -> Result<(Rc<Repo>, bool)>
    where
        S: AsRef<str>,
        O: AsRef<str>,
        N: AsRef<str>,
    {
        match self
            .db
            .get(remote_name.as_ref(), owner_name.as_ref(), name.as_ref())
        {
            Some(repo) => Ok((repo, true)),
            None => self.new_repo(remote_name, owner_name, name),
        }
    }

    fn new_repo<S, O, N>(&self, remote_name: S, owner_name: O, name: N) -> Result<(Rc<Repo>, bool)>
    where
        S: AsRef<str>,
        O: AsRef<str>,
        N: AsRef<str>,
    {
        let repo = Repo::new(
            self.db.cfg,
            remote_name,
            owner_name,
            name,
            self.opts.repo_path.clone(),
            &None,
        )?;
        Ok((repo, false))
    }

    pub fn parse_owner(path: impl AsRef<str>) -> (String, String) {
        let items: Vec<_> = path.as_ref().split("/").collect();
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

    pub fn many_local(&self) -> Result<(Vec<Rc<Repo>>, NameLevel)> {
        let (repos, level) = self.many_local_raw()?;
        if self.opts.many_edit {
            let items: Vec<String> = repos.iter().map(|repo| repo.to_string(&level)).collect();
            let items = self.opts.terminal_helper.edit(items)?;
            let items_set: HashSet<String> = items.into_iter().collect();

            let repos: Vec<Rc<Repo>> = repos
                .into_iter()
                .filter(|repo| items_set.contains(&repo.to_string(&level)))
                .collect();

            return Ok((repos, level));
        }

        Ok((repos, level))
    }

    fn many_local_raw(&self) -> Result<(Vec<Rc<Repo>>, NameLevel)> {
        if self.head.is_empty() {
            let repos = self.db.list_all(&self.opts.filter_labels);
            return Ok((repos, NameLevel::Remote));
        }

        if self.query.is_empty() {
            return match self.db.cfg.get_remote(self.head) {
                Some(_) => {
                    let repos = self.db.list_by_remote(self.head, &self.opts.filter_labels);
                    Ok((repos, NameLevel::Owner))
                }
                None => {
                    let repo = self.db.must_get_fuzzy("", self.head)?;
                    Ok((vec![repo], NameLevel::Owner))
                }
            };
        }

        let remote_name = self.head;
        let remote_cfg = self.db.cfg.must_get_remote(remote_name)?;
        let remote = Remote {
            name: remote_name.to_string(),
            cfg: remote_cfg.clone(),
        };

        if self.query.ends_with("/") {
            let owner = self.query.strip_suffix("/").unwrap();
            return Ok((
                self.db
                    .list_by_owner(remote.name.as_str(), owner, &self.opts.filter_labels),
                NameLevel::Name,
            ));
        }

        let (owner, name) = Self::parse_owner(self.query);
        let repo = if owner.is_empty() {
            self.db.must_get_fuzzy(&remote.name, &name)?
        } else {
            self.db.must_get(&remote.name, &owner, &name)?
        };

        Ok((vec![repo], NameLevel::Name))
    }

    pub fn many_remote(&self) -> Result<(Remote, String, Vec<String>)> {
        let (remote, owner, names) = self.many_remote_raw()?;
        if self.opts.many_edit {
            let names = self.opts.terminal_helper.edit(names.clone())?;
            return Ok((remote, owner, names));
        }

        Ok((remote, owner, names))
    }

    fn many_remote_raw(&self) -> Result<(Remote, String, Vec<String>)> {
        if self.head.is_empty() {
            bail!("internal error, when select many from provider, the head cannot be empty");
        }

        let remote_cfg = self.db.cfg.must_get_remote(self.head)?;
        let remote = Remote {
            name: self.head.to_string(),
            cfg: remote_cfg.clone(),
        };

        let (owner, name) = Self::parse_owner(self.query);
        if !owner.is_empty() && !name.is_empty() {
            return Ok((remote, owner, vec![self.query.to_string()]));
        }

        let owner = match self.query.strip_suffix("/") {
            Some(owner) => owner,
            None => self.query,
        };

        let provider = self.opts.provider_builder.build_provider(
            self.db.cfg,
            &remote,
            self.opts.force_no_cache,
        )?;
        let items = provider.list_repos(owner)?;

        let attached = self
            .db
            .list_by_owner(remote.name.as_str(), owner, &self.opts.filter_labels);
        let attached_set: HashSet<&str> = attached.iter().map(|repo| repo.name.as_str()).collect();

        let names: Vec<_> = items
            .into_iter()
            .filter(|name| !attached_set.contains(name.as_str()))
            .collect();
        Ok((remote, owner.to_string(), names))
    }
}

#[cfg(test)]
mod bucket_tests {
    use crate::config::config_tests;
    use crate::repo::database::*;
    use crate::{hashset, vec_strings};

    fn get_expect_bucket() -> Bucket {
        Bucket {
            remotes: vec_strings!["github", "gitlab"],
            owners: vec_strings!["fioncat", "kubernetes", "test"],
            labels: vec_strings!["pin", "sync"],
            repos: vec![
                RepoBucket {
                    remote: 0,
                    owner: 0,
                    name: String::from("roxide"),
                    accessed: 12.0,
                    last_accessed: 12,
                    path: None,
                    labels: Some(hashset![0]),
                },
                RepoBucket {
                    remote: 0,
                    owner: 0,
                    name: String::from("spacenvim"),
                    accessed: 11.0,
                    last_accessed: 11,
                    path: None,
                    labels: Some(hashset![0]),
                },
                RepoBucket {
                    remote: 0,
                    owner: 1,
                    name: String::from("kubernetes"),
                    accessed: 10.0,
                    last_accessed: 10,
                    path: None,
                    labels: None,
                },
                RepoBucket {
                    remote: 1,
                    owner: 2,
                    name: String::from("test-repo"),
                    accessed: 9.0,
                    last_accessed: 9,
                    path: None,
                    labels: Some(hashset![1]),
                },
            ],
        }
    }

    fn get_expect_repos(cfg: &Config) -> Vec<Rc<Repo>> {
        let github_remote = Rc::new(Remote {
            name: format!("github"),
            cfg: cfg.get_remote("github").unwrap().clone(),
        });
        let gitlab_remote = Rc::new(Remote {
            name: format!("gitlab"),
            cfg: cfg.get_remote("gitlab").unwrap().clone(),
        });
        let fioncat_owner = Rc::new(Owner {
            name: format!("fioncat"),
            remote: Rc::clone(&github_remote),
            cfg: Some(cfg.get_owner("github", "fioncat").unwrap().clone()),
        });
        let kubernetes_owner = Rc::new(Owner {
            name: format!("kubernetes"),
            remote: Rc::clone(&github_remote),
            cfg: Some(cfg.get_owner("github", "kubernetes").unwrap().clone()),
        });
        let test_owner = Rc::new(Owner {
            name: format!("test"),
            remote: Rc::clone(&gitlab_remote),
            cfg: Some(cfg.get_owner("gitlab", "test").unwrap().clone()),
        });

        let pin_label = Rc::new(String::from("pin"));
        let sync_label = Rc::new(String::from("sync"));
        vec![
            Rc::new(Repo {
                name: String::from("roxide"),
                remote: Rc::clone(&github_remote),
                owner: Rc::clone(&fioncat_owner),
                path: None,
                accessed: 12.0,
                last_accessed: 12,
                labels: Some(hashset![Rc::clone(&pin_label)]),
            }),
            Rc::new(Repo {
                name: String::from("spacenvim"),
                remote: Rc::clone(&github_remote),
                owner: Rc::clone(&fioncat_owner),
                path: None,
                accessed: 11.0,
                last_accessed: 11,
                labels: Some(hashset![pin_label]),
            }),
            Rc::new(Repo {
                name: String::from("kubernetes"),
                remote: Rc::clone(&github_remote),
                owner: Rc::clone(&kubernetes_owner),
                path: None,
                accessed: 10.0,
                last_accessed: 10,
                labels: None,
            }),
            Rc::new(Repo {
                name: String::from("test-repo"),
                remote: Rc::clone(&gitlab_remote),
                owner: Rc::clone(&test_owner),
                path: None,
                accessed: 9.0,
                last_accessed: 9,
                labels: Some(hashset![sync_label]),
            }),
        ]
    }

    #[test]
    fn test_bucket_from() {
        let cfg = config_tests::load_test_config("repo_bucket_from");

        let expect_bucket = get_expect_bucket();
        let repos = get_expect_repos(&cfg);

        let bucket = Bucket::from_repos(repos);
        assert_eq!(expect_bucket, bucket);
    }

    #[test]
    fn test_bucket_build() {
        let cfg = config_tests::load_test_config("repo_bucket_build");

        let expect_repos = get_expect_repos(&cfg);

        let bucket = get_expect_bucket();
        let repos = bucket.build_repos(&cfg).unwrap();

        assert_eq!(expect_repos, repos);
    }

    #[test]
    fn test_bucket_convert() {
        let cfg = config_tests::load_test_config("repo_bucket_convert");

        let expect_repos = get_expect_repos(&cfg);
        let repos = get_expect_repos(&cfg);

        let bucket = Bucket::from_repos(repos);
        let repos = bucket.build_repos(&cfg).unwrap();

        assert_eq!(expect_repos, repos);
    }
}

#[cfg(test)]
pub mod database_tests {
    use crate::config::config_tests;
    use crate::hashset_strings;
    use crate::repo::database::*;

    pub fn get_test_repos(cfg: &Config) -> Vec<Rc<Repo>> {
        let mark_labels = Some(hashset_strings!["mark"]);
        vec![
            Repo::new(cfg, "github", "fioncat", "csync", None, &mark_labels).unwrap(),
            Repo::new(cfg, "github", "fioncat", "fioncat", None, &None).unwrap(),
            Repo::new(cfg, "github", "fioncat", "dotfiles", None, &None).unwrap(),
            Repo::new(
                cfg,
                "github",
                "kubernetes",
                "kubernetes",
                None,
                &mark_labels,
            )
            .unwrap(),
            Repo::new(cfg, "github", "kubernetes", "kube-proxy", None, &None).unwrap(),
            Repo::new(cfg, "github", "kubernetes", "kubelet", None, &None).unwrap(),
            Repo::new(cfg, "github", "kubernetes", "kubectl", None, &None).unwrap(),
            Repo::new(cfg, "gitlab", "my-owner-01", "my-repo-01", None, &None).unwrap(),
            Repo::new(cfg, "gitlab", "my-owner-01", "my-repo-02", None, &None).unwrap(),
            Repo::new(cfg, "gitlab", "my-owner-01", "my-repo-03", None, &None).unwrap(),
            Repo::new(cfg, "gitlab", "my-owner-02", "my-repo-01", None, &None).unwrap(),
        ]
    }

    fn repos_as_strings(repos: &Vec<Rc<Repo>>) -> Vec<String> {
        let mut items: Vec<String> = repos.iter().map(|repo| repo.name_with_labels()).collect();
        items.sort();
        items
    }

    fn repos_labels_as_strings(repos: &Vec<Rc<Repo>>, label: &str) -> Vec<String> {
        let repos: Vec<_> = repos
            .iter()
            .filter_map(|repo| match repo.labels.as_ref() {
                Some(labels) => {
                    if labels.contains(&Rc::new(String::from(label))) {
                        Some(Rc::clone(repo))
                    } else {
                        None
                    }
                }
                None => None,
            })
            .collect();
        repos_as_strings(&repos)
    }

    #[test]
    fn test_load() {
        let cfg = config_tests::load_test_config("database/load");
        let repos = get_test_repos(&cfg);

        let mut expect_items: Vec<String> =
            repos.iter().map(|repo| repo.name_with_labels()).collect();
        expect_items.sort();

        let mut db = Database::load(&cfg).unwrap();
        for repo in repos {
            db.update(repo, None);
        }

        db.save().unwrap();

        let db = Database::load(&cfg).unwrap();
        let repos = db.list_all(&None);

        assert_eq!(expect_items, repos_as_strings(&repos));
    }

    #[test]
    fn test_get() {
        let cfg = config_tests::load_test_config("database/get");
        let repos = get_test_repos(&cfg);

        let expect_repos = repos.clone();

        let mut db = Database::load(&cfg).unwrap();
        for repo in repos {
            db.update(repo, None);
        }

        assert_eq!(
            db.get("github", "fioncat", "csync")
                .unwrap()
                .name_with_labels()
                .as_str(),
            "github:fioncat/csync@mark,pin,sync"
        );
        assert_eq!(
            db.get("gitlab", "my-owner-01", "my-repo-01")
                .unwrap()
                .name_with_labels()
                .as_str(),
            "gitlab:my-owner-01/my-repo-01"
        );
        assert_eq!(db.get("gitlab", "my-owner-01", "my-repo"), None);

        for expect_repo in expect_repos {
            let repo = db
                .get(
                    expect_repo.remote.name.as_str(),
                    expect_repo.owner.name.as_str(),
                    expect_repo.name.as_str(),
                )
                .unwrap();
            assert_eq!(repo.name_with_labels(), expect_repo.name_with_labels());
        }
    }

    #[test]
    fn test_list_all() {
        let cfg = config_tests::load_test_config("database/list_all");
        let repos = get_test_repos(&cfg);

        let expect = repos_as_strings(&repos);

        let mut db = Database::load(&cfg).unwrap();
        for repo in repos {
            db.update(repo, None);
        }

        let repos = db.list_all(&None);
        assert_eq!(repos_as_strings(&repos), expect);
    }

    #[test]
    fn test_list_labels() {
        let cfg = config_tests::load_test_config("database/list_labels");
        let repos = get_test_repos(&cfg);

        let expect_pin = repos_labels_as_strings(&repos, "pin");
        let expect_sync = repos_labels_as_strings(&repos, "sync");
        let expect_huge = repos_labels_as_strings(&repos, "huge");
        let expect_mark = repos_labels_as_strings(&repos, "mark");
        let expect_none = repos_labels_as_strings(&repos, "none");

        let mut db = Database::load(&cfg).unwrap();
        for repo in repos {
            db.update(repo, None);
        }

        assert_eq!(
            repos_as_strings(&db.list_all(&Some(hashset_strings!["pin"]))),
            expect_pin
        );
        assert_eq!(
            repos_as_strings(&db.list_all(&Some(hashset_strings!["sync"]))),
            expect_sync
        );
        assert_eq!(
            repos_as_strings(&db.list_all(&Some(hashset_strings!["huge"]))),
            expect_huge
        );
        assert_eq!(
            repos_as_strings(&db.list_all(&Some(hashset_strings!["mark"]))),
            expect_mark
        );
        assert_eq!(
            repos_as_strings(&db.list_all(&Some(hashset_strings!["none"]))),
            expect_none
        );
    }

    #[test]
    fn test_list_remote() {
        let cfg = config_tests::load_test_config("database/list_remote");
        let repos = get_test_repos(&cfg);

        let expect_repos: Vec<_> = repos
            .iter()
            .filter_map(|repo| {
                if repo.remote.name.as_str() == "github" {
                    Some(Rc::clone(repo))
                } else {
                    None
                }
            })
            .collect();

        let mut db = Database::load(&cfg).unwrap();
        for repo in repos {
            db.update(repo, None);
        }

        let repos = db.list_by_remote("github", &None);

        assert_eq!(repos_as_strings(&expect_repos), repos_as_strings(&repos));
    }

    #[test]
    fn test_list_owner() {
        let cfg = config_tests::load_test_config("database/list_owner");
        let repos = get_test_repos(&cfg);

        let expect_repos: Vec<_> = repos
            .iter()
            .filter_map(|repo| {
                if repo.remote.name.as_str() == "github" {
                    if repo.owner.name.as_str() == "fioncat" {
                        Some(Rc::clone(repo))
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect();

        let mut db = Database::load(&cfg).unwrap();
        for repo in repos {
            db.update(repo, None);
        }

        let repos = db.list_by_owner("github", "fioncat", &None);

        assert_eq!(repos_as_strings(&expect_repos), repos_as_strings(&repos));
    }
}

#[cfg(test)]
mod select_tests {
    use crate::api::api_tests::StaticProvider;
    use crate::config::config_tests;
    use crate::repo::database::*;

    #[derive(Debug, Clone)]
    struct TestTerminalHelper {
        target: Option<String>,

        edits: Option<Vec<String>>,
    }

    impl TerminalHelper for TestTerminalHelper {
        fn search(&self, items: &Vec<String>) -> Result<usize> {
            if items.is_empty() {
                bail!("no item to search");
            }
            if let None = self.target {
                return Ok(0);
            }
            let target = self.target.as_ref().unwrap().as_str();

            match items.iter().position(|item| item.as_str() == target) {
                Some(idx) => Ok(idx),
                None => bail!("could not find '{target}'"),
            }
        }

        fn edit(&self, items: Vec<String>) -> Result<Vec<String>> {
            match self.edits.as_ref() {
                Some(edits) => Ok(edits.clone()),
                None => Ok(items),
            }
        }
    }

    #[derive(Debug, Clone)]
    struct StaticProviderBuilder {}

    impl ProviderBuilder for StaticProviderBuilder {
        fn build_provider(
            &self,
            _cfg: &Config,
            _remote: &Remote,
            _force: bool,
        ) -> Result<Box<dyn Provider>> {
            Ok(StaticProvider::mock())
        }
    }

    fn new_test_options(
        target: Option<String>,
        edits: Option<Vec<String>>,
    ) -> SelectOptions<TestTerminalHelper, StaticProviderBuilder> {
        SelectOptions::new(
            TestTerminalHelper { target, edits },
            StaticProviderBuilder {},
        )
    }

    #[test]
    fn test_one_url() {
        let cfg = config_tests::load_test_config("select/one_url");
        let repos = database_tests::get_test_repos(&cfg);

        let cases = vec![
            (
                format!("https://github.com/fioncat/csync"),
                format!("github:fioncat/csync"),
            ),
            (
                format!("https://github.com/fioncat/dotfiles/blob/test/starship.toml"),
                format!("github:fioncat/dotfiles"),
            ),
            (
                format!("https://github.com/kubernetes/kubernetes/tree/master/api/discovery"),
                format!("github:kubernetes/kubernetes"),
            ),
            (
                // The clone https url's ".git" suffix should be trimed.
                format!("https://github.com/kubernetes/kubectl.git"),
                format!("github:kubernetes/kubectl"),
            ),
            (
                // The clone ssh url
                format!("git@github.com:kubernetes/kubectl.git"),
                format!("github:kubernetes/kubectl"),
            ),
            (
                // gitlab format path
                format!("https://gitlab.com/my-owner-01/my-repo-02/-/tree/main.rs"),
                format!("gitlab:my-owner-01/my-repo-02"),
            ),
            (
                // gitlab not found
                format!("https://gitlab.com/my-owner-01/hello/unknown-repo/-/tree/feat/dev"),
                format!("gitlab:my-owner-01/hello/unknown-repo"),
            ),
        ];

        let mut db = Database::load(&cfg).unwrap();
        for repo in repos {
            db.update(repo, None);
        }

        let opts = new_test_options(None, None);
        for case in cases {
            let (url, expect) = case;
            let selector = Selector::new(&db, url.as_str(), "", opts.clone());
            let (repo, exists) = selector.one().unwrap();
            if repo.name.as_str() != "unknown-repo" {
                assert!(exists);
            } else {
                assert!(!exists);
            }
            assert_eq!(expect, repo.name_with_remote());
        }
    }

    #[test]
    fn test_one_fuzzy() {
        let cfg = config_tests::load_test_config("select/one_fuzzy");
        let repos = database_tests::get_test_repos(&cfg);

        let cases = vec![
            (
                format!("github"),
                format!("sync"),
                format!("github:fioncat/csync"),
            ),
            (
                format!(""),
                format!("dot"),
                format!("github:fioncat/dotfiles"),
            ),
            (
                format!("github"),
                format!("proxy"),
                format!("github:kubernetes/kube-proxy"),
            ),
            (
                format!(""),
                format!("let"),
                format!("github:kubernetes/kubelet"),
            ),
            (
                format!(""),
                format!("01"),
                format!("gitlab:my-owner-01/my-repo-01"),
            ),
            (
                format!("gitlab"),
                format!("03"),
                format!("gitlab:my-owner-01/my-repo-03"),
            ),
        ];

        let mut db = Database::load(&cfg).unwrap();
        for repo in repos {
            db.update(repo, None);
        }

        let opts = new_test_options(None, None);
        for case in cases {
            let (remote, keyword, expect) = case;
            let selector = if remote.is_empty() {
                Selector::new(&db, keyword.as_str(), "", opts.clone())
            } else {
                Selector::new(&db, remote.as_str(), keyword.as_str(), opts.clone())
            };
            let (repo, exists) = selector.one().unwrap();
            assert!(exists);
            assert_eq!(expect, repo.name_with_remote());
        }
    }

    #[test]
    fn test_one_search() {
        let cfg = config_tests::load_test_config("select/one_search");
        let repos = database_tests::get_test_repos(&cfg);

        let cases = vec![
            (
                format!(""),
                format!(""),
                format!("github:fioncat/dotfiles"),
                format!("github:fioncat/dotfiles"),
            ),
            (
                format!(""),
                format!(""),
                format!("gitlab:my-owner-01/my-repo-02"),
                format!("gitlab:my-owner-01/my-repo-02"),
            ),
            (
                format!("github"),
                format!(""),
                format!("fioncat/csync"),
                format!("github:fioncat/csync"),
            ),
            (
                format!("github"),
                format!(""),
                format!("kubernetes/kubectl"),
                format!("github:kubernetes/kubectl"),
            ),
            (
                // Search in owner, use format "github fioncat/"
                format!("github"),
                format!("fioncat/"),
                format!("fioncat"),
                format!("github:fioncat/fioncat"),
            ),
            (
                format!("github"),
                format!("kubernetes/"),
                format!("kubelet"),
                format!("github:kubernetes/kubelet"),
            ),
            (
                format!("gitlab"),
                format!("my-owner-02/"),
                format!("my-repo-01"),
                format!("gitlab:my-owner-02/my-repo-01"),
            ),
        ];

        let mut db = Database::load(&cfg).unwrap();
        for repo in repos {
            db.update(repo, None);
        }

        for case in cases {
            let (remote, query, target, expect) = case;
            let opts = new_test_options(Some(target), None)
                .with_force_search(true)
                .with_force_local(true);
            let selector = Selector::new(&db, &remote, &query, opts);
            let (repo, exists) = selector.one().unwrap();
            assert!(exists);
            assert_eq!(expect, repo.name_with_remote());
        }
    }

    #[test]
    fn test_one_latest() {
        let cfg = config_tests::load_test_config("select/one_latest");
        let repos = database_tests::get_test_repos(&cfg);

        let mut db = Database::load(&cfg).unwrap();
        for repo in repos {
            db.update(repo, None);
        }
        let expect = db.must_get_latest("").unwrap();

        let opts = new_test_options(None, None);
        let selector = Selector::new(&db, "", "", opts);
        let (repo, exists) = selector.one().unwrap();
        assert!(exists);
        assert_eq!(repo.name_with_labels(), expect.name_with_labels());
    }

    #[test]
    fn test_one_name_get() {
        let cfg = config_tests::load_test_config("select/one_name_get");
        let repos = database_tests::get_test_repos(&cfg);

        let cases = vec![
            (
                format!("github"),
                format!("fioncat/dotfiles"),
                format!("github:fioncat/dotfiles"),
            ),
            (
                format!("github"),
                format!("kubernetes/kubernetes"),
                format!("github:kubernetes/kubernetes"),
            ),
            (
                format!("github"),
                format!("kubernetes/unknown"),
                format!("github:kubernetes/unknown"),
            ),
        ];
        let mut db = Database::load(&cfg).unwrap();
        for repo in repos {
            db.update(repo, None);
        }

        let opts = new_test_options(None, None);
        for case in cases {
            let (remote, query, expect) = case;
            let selector = Selector::new(&db, &remote, &query, opts.clone());
            let (repo, exists) = selector.one().unwrap();
            if repo.name.as_str() == "unknown" {
                assert!(!exists);
            } else {
                assert!(exists);
            }
            assert_eq!(expect, repo.name_with_remote());
        }
    }
}

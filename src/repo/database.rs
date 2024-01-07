use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::ops::{Deref, DerefMut};
use std::path::PathBuf;
use std::{fs, io};

use anyhow::{bail, Context, Result};
use bincode::Options;
use reqwest::Url;
use serde::{Deserialize, Serialize};

use crate::api::Provider;
use crate::config::{Config, RemoteConfig};
use crate::repo::keywords::Keywords;
use crate::repo::{NameLevel, Repo};
use crate::utils::{self, FileLock};
use crate::{info, term};

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

pub fn backup_replace(cfg: &Config, name: &str) -> Result<()> {
    let old_path = cfg.get_meta_dir().join("database");

    let name = format!("{name}_{}", cfg.now());
    let new_path = cfg.get_meta_dir().join("database_backup").join(&name);
    utils::ensure_dir(&new_path)?;

    fs::rename(&old_path, &new_path).with_context(|| {
        format!(
            "rename '{}' to '{}'",
            old_path.display(),
            new_path.display()
        )
    })?;

    info!("Generate backup database: '{}'", name);
    Ok(())
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct RemoteBucket(HashMap<String, OwnerBucket>);

impl Deref for RemoteBucket {
    type Target = HashMap<String, OwnerBucket>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for RemoteBucket {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct OwnerBucket(HashMap<String, RepoBucket>);

impl Deref for OwnerBucket {
    type Target = HashMap<String, RepoBucket>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for OwnerBucket {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct RepoBucket {
    pub path: Option<String>,
    pub labels: Option<HashSet<u64>>,

    pub last_accessed: u64,
    pub accessed: u64,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Bucket {
    pub data: HashMap<String, RemoteBucket>,

    pub label_index: u64,
    pub labels: HashMap<u64, String>,
}

impl Bucket {
    /// Assume a maximum size for the database. This prevents bincode from
    /// throwing strange errors when it encounters invalid data.
    const MAX_SIZE: u64 = 32 << 20;

    /// Use Version to ensure that decode and encode are consistent.
    pub const VERSION: u32 = 3;

    /// Return empty Bucket, with no repository data.
    fn empty() -> Self {
        Bucket {
            data: HashMap::new(),
            label_index: 0,
            labels: HashMap::new(),
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
}

pub struct Database<'a> {
    cfg: &'a Config,

    bucket: Bucket,

    path: PathBuf,

    /// The Database can only be operated by one process at a time, so file lock
    /// here are used here to provide protection.
    lock: FileLock,

    clean_labels: bool,
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

        Ok(Database {
            cfg,
            bucket,
            path,
            lock,
            clean_labels: false,
        })
    }

    pub fn get<R, O, N>(&self, remote: R, owner: O, name: N) -> Option<Repo>
    where
        R: AsRef<str>,
        O: AsRef<str>,
        N: AsRef<str>,
    {
        let (remote, remote_bucket) = self.bucket.data.get_key_value(remote.as_ref())?;
        let (owner, owner_bucket) = remote_bucket.get_key_value(owner.as_ref())?;
        let (name, repo_bucket) = owner_bucket.get_key_value(name.as_ref())?;

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
            let mut owners: Vec<String> = remote_bucket.keys().map(|key| key.to_string()).collect();
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

        let (owner, mut owner_bucket) =
            remote_bucket.remove_entry(repo.owner.as_ref()).unwrap_or((
                repo.owner.to_string(),
                OwnerBucket(HashMap::with_capacity(1)),
            ));

        let (name, mut repo_bucket) = owner_bucket.remove_entry(repo.name.as_ref()).unwrap_or((
            repo.name.to_string(),
            RepoBucket {
                path: None,
                labels: None,
                last_accessed: 0,
                accessed: 0,
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

        owner_bucket.insert(name, repo_bucket);
        remote_bucket.insert(owner, owner_bucket);
        self.bucket.data.insert(remote, remote_bucket);
    }

    pub fn remove(&mut self, repo: Repo) {
        if let Some((remote, mut remote_bucket)) =
            self.bucket.data.remove_entry(repo.remote.as_ref())
        {
            if let Some((owner, mut owner_bucket)) = remote_bucket.remove_entry(repo.owner.as_ref())
            {
                if let Some(bucket) = owner_bucket.remove(repo.name.as_ref()) {
                    if let Some(_) = bucket.labels {
                        self.clean_labels = true;
                    }
                };
                if !owner_bucket.is_empty() {
                    remote_bucket.insert(owner, owner_bucket);
                }
            }

            if !remote_bucket.is_empty() {
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

    /// Save the database changes to the disk file.
    pub fn save(mut self) -> Result<()> {
        self.do_clean_labels();
        let Database {
            bucket,
            clean_labels: _,
            path,
            lock,
            cfg: _,
        } = self;

        bucket.save(&path)?;
        // Drop lock to release file lock after write done.
        drop(lock);
        Ok(())
    }

    pub fn set_bucket(&mut self, bucket: Bucket) {
        self.bucket = bucket;
    }

    pub fn close(mut self) -> Bucket {
        self.do_clean_labels();
        let Database {
            bucket,
            clean_labels: _,
            path: _,
            lock,
            cfg: _,
        } = self;

        drop(lock);

        bucket
    }

    fn do_clean_labels(&mut self) {
        if !self.clean_labels {
            return;
        }
        let mut use_labels = HashSet::new();
        for (_, remote_bucket) in self.bucket.data.iter() {
            for (_, owner_bucket) in remote_bucket.iter() {
                for (_, repo_bucket) in owner_bucket.iter() {
                    if let Some(labels) = repo_bucket.labels.as_ref() {
                        use_labels.extend(labels.clone());
                    }
                }
            }
        }

        let mut to_remove = HashSet::new();
        for (id, _) in self.bucket.labels.iter() {
            if !use_labels.contains(id) {
                to_remove.insert(*id);
            }
        }

        for id in to_remove {
            self.bucket.labels.remove(&id);
        }
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
        repos.sort_unstable_by(|a, b| {
            let score_a = a.score(self.cfg);
            let score_b = b.score(self.cfg);
            if score_a != score_b {
                return score_b.cmp(&score_a);
            }
            if a.remote != b.remote {
                return a.remote.cmp(&b.remote);
            }
            if a.owner != b.owner {
                return a.owner.cmp(&b.owner);
            }

            a.name.cmp(&b.name)
        })
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
                let (sub_repos, next) = self.scan_remote(remote, remote_bucket, &filter);
                if !next {
                    return Some(sub_repos);
                }
                repos.extend(sub_repos);
            }
            return Some(repos);
        }
        let (remote, remote_bucket) = self.bucket.data.get_key_value(remote.as_ref())?;
        if owner.as_ref().is_empty() {
            let (repos, _) = self.scan_remote(remote, remote_bucket, &filter);
            return Some(repos);
        }

        let (owner, owner_bucket) = remote_bucket.get_key_value(owner.as_ref())?;
        let (repos, _) = self.scan_owner(remote, owner, owner_bucket, &filter);
        Some(repos)
    }

    #[inline]
    fn scan_remote<'a, F>(
        &'a self,
        remote: &'a String,
        remote_bucket: &'a RemoteBucket,
        filter: &F,
    ) -> (Vec<Repo<'a>>, bool)
    where
        F: Fn(&String, &String, &String, &RepoBucket) -> Option<bool>,
    {
        let mut repos = Vec::new();
        for (owner, owner_bucket) in remote_bucket.iter() {
            let (sub_repos, next) = self.scan_owner(remote, owner, owner_bucket, filter);
            if !next {
                return (sub_repos, false);
            }
            repos.extend(sub_repos);
        }
        (repos, true)
    }

    #[inline]
    fn scan_owner<'a, F>(
        &'a self,
        remote: &'a String,
        owner: &'a String,
        owner_bucket: &'a OwnerBucket,
        filter: &F,
    ) -> (Vec<Repo<'a>>, bool)
    where
        F: Fn(&String, &String, &String, &RepoBucket) -> Option<bool>,
    {
        let mut repos = Vec::new();
        for (name, repo_bucket) in owner_bucket.iter() {
            match filter(remote, owner, name, repo_bucket) {
                Some(ok) => {
                    if !ok {
                        continue;
                    }
                }
                None => {
                    return (
                        vec![self.build_repo(remote, owner, name, repo_bucket)],
                        false,
                    );
                }
            }
            let repo = self.build_repo(remote, owner, name, repo_bucket);
            repos.push(repo);
        }
        (repos, true)
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

            remote_cfg: self.cfg.get_remote_or_default(remote),
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

/// Used to provide some functionalities for the terminal. The main purpose of
/// defining this trait is for abstraction, especially for testing purposes where
/// it is inconvenient to directly use some functionalities of the terminal.
pub trait TerminalHelper {
    /// Searching in terminal, typically accomplished by directly invoking `fzf`.
    fn search(&self, items: &Vec<String>) -> Result<usize>;

    /// Use an editor to edit and filter multiple items.
    fn edit(&self, cfg: &Config, items: Vec<String>) -> Result<Vec<String>>;
}

/// Used to construct an API provider, this trait abstraction is primarily for
/// ease of testing.
pub trait ProviderBuilder {
    /// Build an API provider object.
    fn build_provider(
        &self,
        cfg: &Config,
        remote_cfg: &RemoteConfig,
        force: bool,
    ) -> Result<Box<dyn Provider>>;
}

/// The default provider builder implement, use [`crate::api::build_provider`].
pub struct DefaultProviderBuilder {}

impl ProviderBuilder for DefaultProviderBuilder {
    fn build_provider(
        &self,
        cfg: &Config,
        remote_cfg: &RemoteConfig,
        force: bool,
    ) -> Result<Box<dyn Provider>> {
        use crate::api::build_provider;
        build_provider(cfg, remote_cfg, force)
    }
}

/// The default terminal helper implement, use:
///
/// * [`term::fzf_search`] for searching.
/// * [`term::edit_items`] for edit.
pub struct DefaultTerminalHelper {}

impl TerminalHelper for DefaultTerminalHelper {
    fn search(&self, items: &Vec<String>) -> Result<usize> {
        term::fzf_search(items)
    }

    fn edit(&self, cfg: &Config, items: Vec<String>) -> Result<Vec<String>> {
        term::edit_items(cfg, items)
    }
}

/// In certain situations, the [`Selector`] needs to choose from multiple repositories.
/// This enum is used to define whether the preference during selection is towards
/// searching or fuzzy matching.
#[derive(Debug, Clone)]
pub enum SelectMode {
    Fuzzy,
    Search,
}

/// Define options for the [`Selector`] during the selection and filtering process.
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

impl SelectOptions<DefaultTerminalHelper, DefaultProviderBuilder> {
    /// Create a default SelectOptions, with [`DefaultTerminalHelper`]
    /// and [`DefaultProviderBuilder`].
    pub fn default() -> Self {
        SelectOptions::new(DefaultTerminalHelper {}, DefaultProviderBuilder {})
    }
}

impl<T: TerminalHelper, P: ProviderBuilder> SelectOptions<T, P> {
    /// Create a new SelectOptions, with specified [`TerminalHelper`] and [`ProviderBuilder`]
    /// and default options. Useful for testing, in production, please use [`SelectOptions::default`].
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
    pub fn with_filter_labels(mut self, labels: Option<HashSet<String>>) -> Self {
        self.filter_labels = labels;
        self
    }

    /// Search repos from vec
    fn search_from_vec<'a>(&self, mut repos: Vec<Repo<'a>>, level: &NameLevel) -> Result<Repo<'a>> {
        let items: Vec<String> = repos.iter().map(|repo| repo.to_string(&level)).collect();
        let idx = self.terminal_helper.search(&items)?;
        if let None = repos.get(idx) {
            bail!("internal error, terminal_helper returned an invalid index {idx}");
        }
        Ok(repos.remove(idx))
    }
}

enum SelectOneType<'a> {
    Url(Url),
    Ssh(&'a str),
    Direct,
}

/// [`Database`] is responsible for storing basic repository data, but the selection
/// of repositories for operations is handled by a separate Selector.
///
/// The Selector chooses one or more repositories based on the args provided by
/// the user, enabling subsequent operations. The Selector essentially encapsulates
/// the Database, offering more advanced query logic.
///
/// ## Select one
///
/// Please use [`Selector::one`] or [`Selector::must_one`].
///
/// Any query requires two parameters, `head` and `query`. When querying a single
/// repository, their meanings are as follows:
///
/// * `head`: Can have multiple meanings:
///   * If no `query` is provided, it can be the remote name or a keyword for fuzzy
///     searching.
///   * If no `query` is provided and it starts with `http` or `git@`, it represents
///     the clone or access URL of the repository.
///   * If `query` is provided, it forcibly represents the remote name.
/// * `query`: The query statement. It can take different formats:
///   * `{keyword}`: Directly use the keyword to perform fuzzy matching on a
///     repository (if `force_search` is specified, it will use the provider for
///     repository search).
///   * `{owner}/`: Query repositories under a specific `{owner}`.
///   * `{owner}/{name}`: Precisely locate a repository.
/// * If both `head` and `query` are empty, by default, the last accessed repository
///   (excluding the current repository) will be returned. If `force_search` is
///   specified, it will search all local repositories.
///
/// ## Select multiple
///
/// Please use [`Selector::many_local`] or [`Selector::many_remote`].
///
/// For selecting multiple repositories, the parameters are much simpler:
///
/// * `head`: Represents the remote name or fuzzy matching keyword (not support using URL).
/// * `query`: The same as single, but when using `{owner}/`, will select all owner's
///   repositories rather than searching owner.
///
/// In general, when selecting multiple repositories, both of these parameters
/// must be provided.
pub struct Selector<'a, T: TerminalHelper, P: ProviderBuilder> {
    head: &'a str,
    query: &'a str,

    opts: SelectOptions<T, P>,
}

impl<'a, T: TerminalHelper, P: ProviderBuilder> Selector<'_, T, P> {
    const GITHUB_DOMAIN: &'static str = "github.com";

    /// Create a new [`Selector`] object.
    pub fn new(head: &'a str, query: &'a str, opts: SelectOptions<T, P>) -> Selector<'a, T, P> {
        Selector { head, query, opts }
    }

    /// Similar to [`Selector::new`], but use different type for `head` and `query`.
    pub fn from_args(
        head: &'a Option<String>,
        query: &'a Option<String>,
        opts: SelectOptions<T, P>,
    ) -> Selector<'a, T, P> {
        let head = match head {
            Some(head) => head.as_str(),
            None => "",
        };
        let query = match query {
            Some(query) => query.as_str(),
            None => "",
        };

        Self::new(head, query, opts)
    }

    /// Similar to [`Selector::one`], the difference is that an error is returned
    /// if the repository does not exist in the database.
    pub fn must_one<'b>(&self, db: &'b Database) -> Result<Repo<'b>> {
        let (repo, exists) = self.one(db)?;
        if !exists {
            bail!("could not find matched repo");
        }
        Ok(repo)
    }

    /// Select one repository. See [`Selector`].
    ///
    /// # Returns
    ///
    /// Here, there are two return values. The first represents the repository,
    /// and the second indicates whether the repository exists in the database.
    /// How to handle these two parameters is up to the user. For example, the user
    /// may decide to create the repository if it doesn't exist in the database or
    /// raise an error directly.
    ///
    /// See also: [`Selector::must_one`]
    pub fn one<'b>(&self, db: &'b Database) -> Result<(Repo<'b>, bool)> {
        if self.head.is_empty() {
            let repo = match self.opts.mode {
                SelectMode::Fuzzy => db.must_get_latest(""),
                SelectMode::Search => self
                    .opts
                    .search_from_vec(db.list_all(&self.opts.filter_labels), &NameLevel::Remote),
            }?;
            return Ok((repo, true));
        }

        if self.query.is_empty() {
            // If only one `head` parameter is provided, its meaning needs to be
            // inferred based on its format:
            // - It could be a URL.
            // - It could be a clone SSH.
            // - It could be a remote name.
            // - It could be a fuzzy keyword.
            let select_type = if self.head.ends_with(".git") {
                // If `head` ends with `".git"`, by default, consider it as a clone
                // URL, which could be in either HTTP or SSH format. However, it
                // could also be just a remote or keyword ending with `".git"`.
                if self.head.starts_with("http") {
                    let url = self.head.strip_suffix(".git").unwrap();
                    SelectOneType::Url(
                        Url::parse(&url).with_context(|| format!("parse clone url '{url}'"))?,
                    )
                } else if self.head.starts_with("git@") {
                    SelectOneType::Ssh(self.head)
                } else {
                    // If it is neither HTTP nor SSH, consider it not to be a
                    // clone URL.
                    match Url::parse(self.head) {
                        Ok(url) => SelectOneType::Url(url),
                        Err(_) => SelectOneType::Direct,
                    }
                }
            } else {
                // We prioritize treating `head` as a URL, so we attempt to parse
                // it using URL rules. If parsing fails, then consider it as a
                // remote or keyword.
                match Url::parse(&self.head) {
                    Ok(url) => SelectOneType::Url(url),
                    Err(_) => SelectOneType::Direct,
                }
            };

            return match select_type {
                SelectOneType::Url(url) => self.one_from_url(db, url),
                SelectOneType::Ssh(ssh) => self.one_from_ssh(db, ssh),
                SelectOneType::Direct => self.one_from_head(db),
            };
        }

        self.one_from_owner(db)
    }

    /// Select one repository from a url.
    fn one_from_url<'b>(&self, db: &'b Database, url: Url) -> Result<(Repo<'b>, bool)> {
        let domain = match url.domain() {
            Some(domain) => domain,
            None => bail!("missing domain in url '{url}'"),
        };
        let remotes = db.cfg.list_remotes();

        let mut target_remote: Option<String> = None;
        let mut is_gitlab = false;
        for remote in remotes {
            // We match the domain of the URL based on the clone domain of the
            // remote. This is because in many cases, the provided URL is a clone
            // address, and even for access addresses, most of the time their
            // domains are consistent with the clone.
            // TODO: Provide another match domain in config?
            let remote_cfg = db.cfg.must_get_remote(&remote)?;
            let remote_domain = match remote_cfg.clone.as_ref() {
                Some(domain) => domain,
                None => continue,
            };

            if remote_domain != domain {
                continue;
            }

            // We only support parsing two types of URLs: GitHub and GitLab. For
            // non-GitHub cases, we consider them all as GitLab.
            // TODO: Add support for parsing URLs from more types of remotes.
            if remote_domain != Self::GITHUB_DOMAIN {
                is_gitlab = true;
            }
            target_remote = Some(remote);
            break;
        }

        let remote = match target_remote {
            Some(remote) => remote,
            None => bail!("could not find remote whose clone domain is '{domain}' in url"),
        };

        let path_iter = match url.path_segments() {
            Some(iter) => iter,
            None => bail!("invalid url '{url}', path could not be empty"),
        };

        // We use a simple method to parse repository URL:
        //
        // - For GitHub, both owner and name are required, and sub-owners are not
        // supported. Therefore, as long as two path segments are identified, it
        // is considered within a repository. The subsequent path is assumed to be
        // the branch or file path.
        //
        // - For GitLab, the presence of sub-owners complicates direct localization
        // of two segments. The path rule in GitLab is that starting from "-", the
        // subsequent path is the branch or file. Therefore, locating the "-" is
        // sufficient for GitLab.
        let mut segs = Vec::new(); // The segments for repository, contains owner and name
        for part in path_iter {
            if is_gitlab {
                if part == "-" {
                    break;
                }
                segs.push(part);
                continue;
            }

            if segs.len() == 2 {
                break;
            }
            segs.push(part);
        }

        // The owner and name are both required for GitHub and GitLab, so the length
        // of `segs` should be bigger than 2.
        // If not, it means that user are not in a repository, maybe in an owner.
        if segs.len() < 2 {
            bail!("invalid url '{url}', should be in a repo");
        }

        let path = segs.join("/");
        let (mut owner, mut name) = parse_owner(&path);

        let remote_cfg = db.cfg.must_get_remote(&remote)?;
        if remote_cfg.has_alias() {
            // We need to convert the owner and name in the URL to alias name.
            // Note that only the URL requires this processing, as URLs are typically directly
            // copied by users from the browser and their owner and name are usually in their
            // original form.
            if let Some(owner_cfg) = remote_cfg.owners.get(&owner) {
                if let Some(owner_alias) = owner_cfg.alias.as_ref() {
                    owner = owner_alias.to_string();
                }
                if let Some(name_alias) = owner_cfg.repo_alias.get(&name) {
                    name = name_alias.to_string();
                }
            }
        }

        match db.get(&remote, &owner, &name) {
            Some(repo) => Ok((repo, true)),
            None => self.new_repo(db, &remote, &owner, &name),
        }
    }

    /// Select one repository from a ssh url.
    fn one_from_ssh<'b>(&self, db: &'b Database, ssh: &str) -> Result<(Repo<'b>, bool)> {
        // Parsing SSH is done in a clever way by reusing the code for parsing
        // URLs. The approach involves converting the SSH statement to a URL and
        // then calling the URL parsing code.
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

        self.one_from_url(db, url)
            .with_context(|| format!("select from ssh '{ssh}'"))
    }

    /// Select one repository from a head statement.
    fn one_from_head<'b>(&self, db: &'b Database) -> Result<(Repo<'b>, bool)> {
        // Treating `head` as a remote (with higher priority) or fuzzy matching
        // keyword, we will call different functions from the database to retrieve
        // the information.
        match db.cfg.get_remote(self.head) {
            Some(_) => {
                let repo = match self.opts.mode {
                    SelectMode::Search => self.opts.search_from_vec(
                        db.list_by_remote(self.head, &self.opts.filter_labels),
                        &NameLevel::Owner,
                    ),
                    SelectMode::Fuzzy => db.must_get_latest(self.head),
                }?;
                Ok((repo, true))
            }
            None => self.fuzzy_get_repo(db, "", self.head),
        }
    }

    /// Select one repository from owner.
    fn one_from_owner<'b>(&self, db: &'b Database) -> Result<(Repo<'b>, bool)> {
        // Up to this point, with the `query` provided, indicating that `head`
        // represents a remote, we can directly retrieve the remote configuration.
        let remote = self.head;
        let remote_cfg = db.cfg.must_get_remote(remote)?;

        // A special syntax: If `query` ends with "/", it indicates a search
        // within the owner.
        if self.query.ends_with("/") {
            let owner = self.query.strip_suffix("/").unwrap();
            let mut search_local = self.opts.force_local;
            if let None = remote_cfg.provider {
                search_local = true;
            }

            if search_local {
                let repo = self.opts.search_from_vec(
                    db.list_by_owner(remote, owner, &self.opts.filter_labels),
                    &NameLevel::Name,
                )?;
                return Ok((repo, true));
            }

            let provider = self.opts.provider_builder.build_provider(
                &db.cfg,
                &remote_cfg,
                self.opts.force_no_cache,
            )?;
            let mut api_repos = provider.list_repos(owner)?;

            if self.opts.force_remote {
                // force remote, don't show exists repos
                let owner_repos = db.list_by_owner(remote, owner, &None);
                let repos_set: HashSet<&str> =
                    owner_repos.iter().map(|repo| repo.name.as_ref()).collect();
                api_repos = api_repos
                    .into_iter()
                    .filter(|name| !repos_set.contains(name.as_str()))
                    .collect();
            }

            let idx = self.opts.terminal_helper.search(&api_repos)?;
            let name = &api_repos[idx];
            return self.get_or_create_repo(db, remote, owner, name);
        }

        // At this point, there are still two potential branching scenarios:
        //
        // - `query` might be a fuzzy matching keyword or a keyword for searching.
        // This can be determined based on whether `query` contains "/".
        // - If `query` contains "/", the user wants to directly locate or
        // create a repository, and we can directly call the get function.
        let (owner, name) = parse_owner(self.query);
        if owner.is_empty() {
            return match self.opts.mode {
                SelectMode::Search => self.one_from_provider(db, &remote_cfg),
                SelectMode::Fuzzy => self.fuzzy_get_repo(db, remote, self.query),
            };
        }

        self.get_or_create_repo(db, remote, &owner, &name)
    }

    /// Select one repository from remote provider.
    fn one_from_provider<'b>(
        &self,
        db: &'b Database,
        remote_cfg: &RemoteConfig,
    ) -> Result<(Repo<'b>, bool)> {
        let provider = self.opts.provider_builder.build_provider(
            &db.cfg,
            remote_cfg,
            self.opts.force_no_cache,
        )?;

        let items = provider.search_repos(self.query)?;
        if items.is_empty() {
            bail!("no result found from remote");
        }

        let idx = self.opts.terminal_helper.search(&items)?;
        let result = &items[idx];

        let (owner, name) = parse_owner(result);
        if owner.is_empty() || name.is_empty() {
            bail!("invalid repo name '{result}' from remote");
        }

        self.get_or_create_repo(db, remote_cfg.get_name(), &owner, &name)
    }

    /// Creating or retrieving a repository, the second return value indicates
    /// whether the repository exists in the Database.
    fn get_or_create_repo<'b, S, O, N>(
        &self,
        db: &'b Database,
        remote: S,
        owner: O,
        name: N,
    ) -> Result<(Repo<'b>, bool)>
    where
        S: AsRef<str>,
        O: AsRef<str>,
        N: AsRef<str>,
    {
        match db.get(remote.as_ref(), owner.as_ref(), name.as_ref()) {
            Some(repo) => Ok((repo, true)),
            None => self.new_repo(db, remote, owner, name),
        }
    }

    /// Create a new repository object.
    fn new_repo<'b, S, O, N>(
        &self,
        db: &'b Database,
        remote: S,
        owner: O,
        name: N,
    ) -> Result<(Repo<'b>, bool)>
    where
        S: AsRef<str>,
        O: AsRef<str>,
        N: AsRef<str>,
    {
        let repo = Repo::new(
            db.cfg,
            Cow::Owned(remote.as_ref().to_string()),
            Cow::Owned(owner.as_ref().to_string()),
            Cow::Owned(name.as_ref().to_string()),
            self.opts.repo_path.clone(),
        )?;
        Ok((repo, false))
    }

    /// Wrap fuzzy get, add keywords addon.
    fn fuzzy_get_repo<'b, R, K>(
        &self,
        db: &'b Database,
        remote: R,
        keyword: K,
    ) -> Result<(Repo<'b>, bool)>
    where
        R: AsRef<str>,
        K: AsRef<str>,
    {
        let repo = db.must_get_fuzzy(remote.as_ref(), keyword.as_ref())?;

        if repo.name != keyword.as_ref() {
            // If a fuzzy match hits a repository, record the fuzzy matching keywords in a
            // file for automatic keyword completion. If the fuzzy match word exactly matches
            // the repository name, no additional recording is needed, as the completion
            // logic will automatically include the repository name in the completion
            // candidates.
            let mut keywords = Keywords::load(db.cfg)?;
            keywords.upsert(remote.as_ref(), keyword.as_ref());
            keywords.save()?;
        }

        Ok((repo, true))
    }

    /// Selecting multiple repositories from the local database.
    ///
    /// # Returns
    ///
    /// In addition to the list of repositories, the return value will also include
    /// a suggested [`NameLevel`] for display.
    /// If you want to display the list of repositories, use this [`NameLevel`] to
    /// call [`Repo::to_string`].
    pub fn many_local<'b>(&self, db: &'b Database) -> Result<(Vec<Repo<'b>>, NameLevel)> {
        let (repos, level) = self.many_local_raw(db)?;
        if self.opts.many_edit {
            let items: Vec<String> = repos.iter().map(|repo| repo.to_string(&level)).collect();
            let items = self.opts.terminal_helper.edit(db.cfg, items)?;
            let items_set: HashSet<String> = items.into_iter().collect();

            let repos: Vec<Repo> = repos
                .into_iter()
                .filter(|repo| items_set.contains(&repo.to_string(&level)))
                .collect();

            return Ok((repos, level));
        }

        Ok((repos, level))
    }

    /// The same as [`Selector::many_local`], without editor filtering.
    fn many_local_raw<'b>(&self, db: &'b Database) -> Result<(Vec<Repo<'b>>, NameLevel)> {
        if self.head.is_empty() {
            // List all repositories.
            let repos = db.list_all(&self.opts.filter_labels);
            return Ok((repos, NameLevel::Remote));
        }

        if self.query.is_empty() {
            // If `head` is the remote name, select all repositories under that
            // remote; otherwise, perform a fuzzy search.
            return match db.cfg.get_remote(self.head) {
                Some(_) => {
                    let repos = db.list_by_remote(self.head, &self.opts.filter_labels);
                    Ok((repos, NameLevel::Owner))
                }
                None => {
                    let (repo, _) = self.fuzzy_get_repo(db, "", self.head)?;
                    Ok((vec![repo], NameLevel::Owner))
                }
            };
        }

        // When selecting multiple repositories, the logic here is similar to
        // selecting one. Adding "/" after the query indicates selecting the entire
        // owner, and not adding it uses fuzzy matching. The difference from
        // selecting one is that there is no search performed here.
        let remote = self.head;
        let _ = db.cfg.must_get_remote(remote)?;

        if self.query.ends_with("/") {
            let owner = self.query.strip_suffix("/").unwrap();
            return Ok((
                db.list_by_owner(remote, owner, &self.opts.filter_labels),
                NameLevel::Name,
            ));
        }

        let (owner, name) = parse_owner(self.query);
        let repo = if owner.is_empty() {
            let (repo, _) = self.fuzzy_get_repo(db, remote, &name)?;
            repo
        } else {
            db.must_get(remote, &owner, &name)?
        };

        Ok((vec![repo], NameLevel::Name))
    }

    /// Selecting multiple repositories from the remote provider.
    ///
    /// # Returns
    ///
    /// - [`Remote`]: The remote object being searched.
    /// - String: The owner name being searched.
    /// - Vec<String>: The search results.
    pub fn many_remote(&self, db: &Database) -> Result<(RemoteConfig, String, Vec<String>)> {
        let (remote_cfg, owner, names) = self.many_remote_raw(db)?;
        if self.opts.many_edit {
            let names = self.opts.terminal_helper.edit(db.cfg, names.clone())?;
            return Ok((remote_cfg, owner, names));
        }

        Ok((remote_cfg, owner, names))
    }

    /// The same as [`Selector::many_local`], without editor filtering.
    fn many_remote_raw(&self, db: &Database) -> Result<(RemoteConfig, String, Vec<String>)> {
        if self.head.is_empty() {
            bail!("internal error, when select many from provider, the head cannot be empty");
        }

        let remote = self.head;
        let remote_cfg = db.cfg.must_get_remote(self.head)?;
        let remote_cfg = remote_cfg.into_owned();

        let (owner, name) = parse_owner(self.query);
        if !owner.is_empty() && !name.is_empty() {
            return Ok((remote_cfg, owner, vec![self.query.to_string()]));
        }

        let owner = self.query.strip_suffix("/").unwrap_or_else(|| self.query);

        let provider = self.opts.provider_builder.build_provider(
            db.cfg,
            &remote_cfg,
            self.opts.force_no_cache,
        )?;
        let items = provider.list_repos(owner)?;

        let attached = db.list_by_owner(remote, owner, &self.opts.filter_labels);
        let attached_set: HashSet<&str> = attached.iter().map(|repo| repo.name.as_ref()).collect();

        let names: Vec<_> = items
            .into_iter()
            .filter(|name| !attached_set.contains(name.as_str()))
            .collect();
        Ok((remote_cfg, owner.to_string(), names))
    }
}

/// Parsing a path into owner and name follows the basic format: `"{owner}/{name}"`.
/// `"{owner}"` adheres to GitLab's rules and can include sub-owners (i.e., multiple
/// levels of directories). If the path does not contain `"/"`, then return the
/// path directly with an empty owner.
///
/// # Examples
///
/// ```
/// assert_eq!(Selector::parse_owner("fioncat/roxide"), (format!("fioncat"), format!("roxide")));
/// assert_eq!(Selector::parse_owner("s0/s1/repo"), (format!("s0/s1"), format!("repo")));
/// assert_eq!(Selector::parse_owner("roxide"), (format!(""), format!("roxide")));
/// ```
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

#[cfg(test)]
pub mod database_tests {
    use crate::config::config_tests;
    use crate::repo::database::*;
    use crate::{hashset, hashset_strings};

    fn new_test_repo<'a>(
        cfg: &'a Config,
        remote: &'static str,
        owner: &'static str,
        name: &'static str,
        labels: Option<Vec<&'static str>>,
    ) -> Repo<'a> {
        let labels = match labels {
            Some(labels) => Some(
                labels
                    .into_iter()
                    .map(|label| Cow::Borrowed(label))
                    .collect(),
            ),
            None => None,
        };
        Repo {
            remote: Cow::Borrowed(remote),
            owner: Cow::Borrowed(owner),
            name: Cow::Borrowed(name),
            last_accessed: 0,
            accessed: 0,

            remote_cfg: cfg.get_remote_or_default(remote),
            owner_cfg: cfg.get_owner(remote, owner),

            labels,
            path: None,
        }
    }

    pub fn get_test_repos(cfg: &Config) -> Vec<Repo> {
        vec![
            new_test_repo(cfg, "github", "fioncat", "csync", Some(vec!["sync", "pin"])),
            new_test_repo(cfg, "github", "fioncat", "fioncat", Some(vec!["sync"])),
            new_test_repo(cfg, "github", "fioncat", "dotfiles", Some(vec!["sync"])),
            new_test_repo(
                cfg,
                "github",
                "kubernetes",
                "kubernetes",
                Some(vec!["mark", "sync"]),
            ),
            new_test_repo(cfg, "github", "kubernetes", "kube-proxy", None),
            new_test_repo(cfg, "github", "kubernetes", "kubelet", None),
            new_test_repo(cfg, "github", "kubernetes", "kubectl", Some(vec!["mark"])),
            new_test_repo(
                cfg,
                "gitlab",
                "my-owner-01",
                "my-repo-01",
                Some(vec!["sync"]),
            ),
            new_test_repo(
                cfg,
                "gitlab",
                "my-owner-01",
                "my-repo-02",
                Some(vec!["mark"]),
            ),
            new_test_repo(cfg, "gitlab", "my-owner-01", "my-repo-03", None),
            new_test_repo(cfg, "gitlab", "my-owner-02", "my-repo-01", None),
        ]
    }

    #[test]
    fn test_load() {
        let cfg = config_tests::load_test_config("database/load");
        let repos = get_test_repos(&cfg);
        let mut expect = repos.clone();

        let mut db = Database::load(&cfg).unwrap();
        for repo in repos {
            db.upsert(repo);
        }

        db.sort_repos(&mut expect);
        db.save().unwrap();

        let db = Database::load(&cfg).unwrap();
        let repos = db.list_all(&None);

        assert_eq!(repos, expect);
    }

    #[test]
    fn test_get() {
        let cfg = config_tests::load_test_config("database/get");
        let repos = get_test_repos(&cfg);

        let mut db = Database::load(&cfg).unwrap();
        for repo in repos {
            db.upsert(repo);
        }

        assert_eq!(
            db.get("github", "fioncat", "csync").unwrap(),
            new_test_repo(
                &cfg,
                "github",
                "fioncat",
                "csync",
                Some(vec!["sync", "pin"])
            )
        );
        assert_eq!(db.get("github", "fioncat", "ghost"), None);
        assert_eq!(
            db.get("gitlab", "my-owner-01", "my-repo-02").unwrap(),
            new_test_repo(
                &cfg,
                "gitlab",
                "my-owner-01",
                "my-repo-02",
                Some(vec!["mark"])
            )
        );

        db.save().unwrap();
    }

    #[test]
    fn test_fuzzy() {
        let cfg = config_tests::load_test_config("database/fuzzy");
        let repos = get_test_repos(&cfg);

        let mut db = Database::load(&cfg).unwrap();
        for repo in repos {
            db.upsert(repo);
        }

        assert_eq!(
            db.get_fuzzy("", "dot").unwrap(),
            new_test_repo(&cfg, "github", "fioncat", "dotfiles", Some(vec!["sync"]))
        );
        assert_eq!(db.get_fuzzy("gitlab", "dot"), None);

        let mut update_repo = new_test_repo(&cfg, "github", "kubernetes", "kubectl", None);
        update_repo.accessed = 100; // Make this repo the highest score
        db.upsert(update_repo.clone());
        assert_eq!(db.get_fuzzy("", "kube").unwrap(), update_repo);
    }

    #[test]
    fn test_latest() {
        let cfg = config_tests::load_test_config("database/latest");
        let repos = get_test_repos(&cfg);

        let mut db = Database::load(&cfg).unwrap();
        for repo in repos {
            db.upsert(repo);
        }

        let mut repo = new_test_repo(&cfg, "github", "kubernetes", "kube-proxy", None);
        repo.last_accessed = cfg.now() + 200;
        db.upsert(repo.clone());
        assert_eq!(db.get_latest("").unwrap(), repo);
        let latest = repo;

        let mut repo = new_test_repo(&cfg, "gitlab", "my-owner-01", "my-repo-03", None);
        repo.last_accessed = cfg.now() + 10;
        db.upsert(repo.clone());
        assert_eq!(db.get_latest("gitlab").unwrap(), repo);
        assert_eq!(db.get_latest("").unwrap(), latest);

        let latest = repo;

        let dir = cfg.get_current_dir().parent().unwrap();
        let mut repo = new_test_repo(&cfg, "github", "kubernetes", "kube-proxy", None);
        repo.path = Some(Cow::Owned(format!("{}", dir.display())));
        db.upsert(repo.clone());
        assert_eq!(db.get_latest("").unwrap(), repo);
        assert_eq!(db.get_latest("gitlab").unwrap(), latest);

        repo.path = Some(Cow::Owned(format!("{}", cfg.get_current_dir().display())));
        repo.accessed = 5000;
        db.upsert(repo.clone());
        assert_eq!(db.get_latest("").unwrap(), latest);
    }

    #[test]
    fn list_by_remote() {
        let cfg = config_tests::load_test_config("database/list_by_remote");
        let repos = get_test_repos(&cfg);

        let mut expect: Vec<_> = repos
            .iter()
            .filter(|repo| repo.remote.as_ref() == "github")
            .map(|repo| repo.clone())
            .collect();

        let mut db = Database::load(&cfg).unwrap();
        for repo in repos {
            db.upsert(repo);
        }

        db.sort_repos(&mut expect);

        assert_eq!(db.list_by_remote("github", &None), expect);

        let repos = db.list_by_remote("github", &Some(hashset_strings!["pin"]));
        assert_eq!(
            repos,
            vec![new_test_repo(
                &cfg,
                "github",
                "fioncat",
                "csync",
                Some(vec!["sync", "pin"])
            )]
        );
        assert_eq!(
            db.list_by_remote("gitlab", &Some(hashset_strings!["pin"])),
            vec![]
        );
    }

    #[test]
    fn list_by_owner() {
        let cfg = config_tests::load_test_config("database/list_by_owner");
        let repos = get_test_repos(&cfg);

        let mut expect: Vec<_> = repos
            .iter()
            .filter(|repo| repo.remote.as_ref() == "github" && repo.owner.as_ref() == "kubernetes")
            .map(|repo| repo.clone())
            .collect();

        let mut db = Database::load(&cfg).unwrap();
        for repo in repos {
            db.upsert(repo);
        }

        db.sort_repos(&mut expect);

        assert_eq!(db.list_by_owner("github", "kubernetes", &None), expect);
        let mut mark_repos = vec![
            new_test_repo(
                &cfg,
                "github",
                "kubernetes",
                "kubernetes",
                Some(vec!["mark", "sync"]),
            ),
            new_test_repo(&cfg, "github", "kubernetes", "kubectl", Some(vec!["mark"])),
        ];
        db.sort_repos(&mut mark_repos);
        assert_eq!(
            db.list_by_owner("github", "kubernetes", &Some(hashset_strings!["mark"])),
            mark_repos
        );
    }

    #[test]
    fn upsert() {
        let cfg = config_tests::load_test_config("database/upsert");
        let repos = get_test_repos(&cfg);

        let mut db = Database::load(&cfg).unwrap();
        for repo in repos {
            db.upsert(repo);
        }

        db.upsert(new_test_repo(
            &cfg,
            "github",
            "kubernetes",
            "kubernetes",
            None,
        ));
        db.upsert(new_test_repo(&cfg, "github", "kubernetes", "kubectl", None));
        db.upsert(new_test_repo(
            &cfg,
            "gitlab",
            "my-owner-01",
            "my-repo-02",
            None,
        ));

        assert!(db.clean_labels);
        db.save().unwrap();

        let db = Database::load(&cfg).unwrap();
        let labels: HashSet<&str> = db
            .bucket
            .labels
            .iter()
            .map(|(_id, label)| label.as_str())
            .collect();
        // The mark label should be removed
        assert_eq!(labels, hashset!["sync", "pin"]);
    }

    #[test]
    fn remove() {
        let cfg = config_tests::load_test_config("database/remove");
        let repos = get_test_repos(&cfg);

        let mut db = Database::load(&cfg).unwrap();
        for repo in repos {
            db.upsert(repo);
        }

        db.remove(new_test_repo(
            &cfg,
            "github",
            "kubernetes",
            "kubernetes",
            None,
        ));
        db.remove(new_test_repo(&cfg, "github", "kubernetes", "kubectl", None));
        db.remove(new_test_repo(
            &cfg,
            "gitlab",
            "my-owner-01",
            "my-repo-02",
            None,
        ));

        assert_eq!(db.get("github", "kubernetes", "kubernetes"), None);
        assert_eq!(db.get("github", "kubernetes", "kubectl"), None);
        assert_eq!(db.get("gitlab", "my-owner-01", "my-repo-02"), None);
        assert!(db.clean_labels);

        db.save().unwrap();

        let db = Database::load(&cfg).unwrap();
        let labels: HashSet<&str> = db
            .bucket
            .labels
            .iter()
            .map(|(_id, label)| label.as_str())
            .collect();
        // The mark label should be removed
        assert_eq!(labels, hashset!["sync", "pin"]);
    }
}

#[cfg(test)]
mod select_tests {
    use crate::api::api_tests::StaticProvider;
    use crate::config::config_tests;
    use crate::hashset_strings;
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

        fn edit(&self, _cfg: &Config, items: Vec<String>) -> Result<Vec<String>> {
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
            _remote: &RemoteConfig,
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
                "https://github.com/fioncat/csync".to_string(),
                "github:fioncat/csync".to_string(),
            ),
            (
                "https://github.com/fioncat/dotfiles/blob/test/starship.toml".to_string(),
                "github:fioncat/dotfiles".to_string(),
            ),
            (
                // The clone ssh url
                "git@github.com:fioncat/fioncat.git".to_string(),
                "github:fioncat/fioncat".to_string(),
            ),
            (
                // gitlab format path
                "https://gitlab.com/my-owner-01/my-repo-02/-/tree/main.rs".to_string(),
                "gitlab:my-owner-01/my-repo-02".to_string(),
            ),
            (
                // gitlab not found
                "https://gitlab.com/my-owner-01/hello/unknown-repo/-/tree/feat/dev".to_string(),
                "gitlab:my-owner-01/hello/unknown-repo".to_string(),
            ),
            // The alias will be expanded
            (
                "https://github.com/kubernetes/kubernetes/tree/master/api/discovery".to_string(),
                "github:k8s/k8s".to_string(),
            ),
            (
                "https://github.com/kubernetes/kubectl.git".to_string(),
                "github:k8s/kubectl".to_string(),
            ),
            (
                "git@github.com:kubernetes/kubectl.git".to_string(),
                "github:k8s/kubectl".to_string(),
            ),
        ];

        let mut db = Database::load(&cfg).unwrap();
        for repo in repos {
            db.upsert(repo);
        }

        let opts = new_test_options(None, None);
        for case in cases {
            let (url, expect) = case;
            let selector = Selector::new(url.as_str(), "", opts.clone());
            let (repo, exists) = selector.one(&db).unwrap();
            if repo.name.as_ref() != "unknown-repo" {
                if repo.owner.as_ref() == "k8s" {
                    assert!(!exists);
                } else {
                    assert!(exists);
                }
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
                "github".to_string(),
                "sync".to_string(),
                "github:fioncat/csync".to_string(),
            ),
            (
                "".to_string(),
                "dot".to_string(),
                "github:fioncat/dotfiles".to_string(),
            ),
            (
                "github".to_string(),
                "proxy".to_string(),
                "github:kubernetes/kube-proxy".to_string(),
            ),
            (
                "".to_string(),
                "let".to_string(),
                "github:kubernetes/kubelet".to_string(),
            ),
            (
                "gitlab".to_string(),
                "03".to_string(),
                "gitlab:my-owner-01/my-repo-03".to_string(),
            ),
        ];

        let mut db = Database::load(&cfg).unwrap();
        for repo in repos {
            db.upsert(repo);
        }

        let opts = new_test_options(None, None);
        for case in cases {
            let (remote, keyword, expect) = case;
            let selector = if remote.is_empty() {
                Selector::new(keyword.as_str(), "", opts.clone())
            } else {
                Selector::new(remote.as_str(), keyword.as_str(), opts.clone())
            };
            let (repo, exists) = selector.one(&db).unwrap();
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
                "".to_string(),
                "".to_string(),
                "github:fioncat/dotfiles".to_string(),
                "github:fioncat/dotfiles".to_string(),
            ),
            (
                "".to_string(),
                "".to_string(),
                "gitlab:my-owner-01/my-repo-02".to_string(),
                "gitlab:my-owner-01/my-repo-02".to_string(),
            ),
            (
                "github".to_string(),
                "".to_string(),
                "fioncat/csync".to_string(),
                "github:fioncat/csync".to_string(),
            ),
            (
                "github".to_string(),
                "".to_string(),
                "kubernetes/kubectl".to_string(),
                "github:kubernetes/kubectl".to_string(),
            ),
            (
                // Search in owner, use format "github fioncat/"
                "github".to_string(),
                "fioncat/".to_string(),
                "fioncat".to_string(),
                "github:fioncat/fioncat".to_string(),
            ),
            (
                "github".to_string(),
                "kubernetes/".to_string(),
                "kubelet".to_string(),
                "github:kubernetes/kubelet".to_string(),
            ),
            (
                "gitlab".to_string(),
                "my-owner-02/".to_string(),
                "my-repo-01".to_string(),
                "gitlab:my-owner-02/my-repo-01".to_string(),
            ),
        ];

        let mut db = Database::load(&cfg).unwrap();
        for repo in repos {
            db.upsert(repo);
        }

        for case in cases {
            let (remote, query, target, expect) = case;
            let opts = new_test_options(Some(target), None)
                .with_force_search(true)
                .with_force_local(true);
            let selector = Selector::new(&remote, &query, opts);
            let (repo, exists) = selector.one(&db).unwrap();
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
            db.upsert(repo);
        }
        let expect = db.must_get_latest("").unwrap();

        let opts = new_test_options(None, None);
        let selector = Selector::new("", "", opts);
        let (repo, exists) = selector.one(&db).unwrap();
        assert!(exists);
        assert_eq!(repo.name_with_labels(), expect.name_with_labels());
    }

    #[test]
    fn test_one_name_get() {
        let cfg = config_tests::load_test_config("select/one_name_get");
        let repos = database_tests::get_test_repos(&cfg);

        let cases = vec![
            (
                "github".to_string(),
                "fioncat/dotfiles".to_string(),
                "github:fioncat/dotfiles".to_string(),
            ),
            (
                "github".to_string(),
                "kubernetes/kubernetes".to_string(),
                "github:kubernetes/kubernetes".to_string(),
            ),
            (
                "github".to_string(),
                "kubernetes/unknown".to_string(),
                "github:kubernetes/unknown".to_string(),
            ),
        ];
        let mut db = Database::load(&cfg).unwrap();
        for repo in repos {
            db.upsert(repo);
        }

        let opts = new_test_options(None, None);
        for case in cases {
            let (remote, query, expect) = case;
            let selector = Selector::new(&remote, &query, opts.clone());
            let (repo, exists) = selector.one(&db).unwrap();
            if repo.name.as_ref() == "unknown" {
                assert!(!exists);
            } else {
                assert!(exists);
            }
            assert_eq!(expect, repo.name_with_remote());
        }
    }

    #[test]
    fn test_many_local() {
        let cfg = config_tests::load_test_config("select/many_local");
        let repos = database_tests::get_test_repos(&cfg);
        let mut db = Database::load(&cfg).unwrap();
        for repo in repos {
            db.upsert(repo);
        }

        let cases = vec![
            (
                "".to_string(),
                "".to_string(),
                db.list_all(&None)
                    .iter()
                    .map(|repo| repo.name_with_labels())
                    .collect::<HashSet<String>>(),
            ),
            (
                "github".to_string(),
                "".to_string(),
                db.list_by_remote("github", &None)
                    .iter()
                    .map(|repo| repo.name_with_labels())
                    .collect::<HashSet<String>>(),
            ),
            (
                "github".to_string(),
                "fioncat/".to_string(),
                db.list_by_owner("github", "fioncat", &None)
                    .iter()
                    .map(|repo| repo.name_with_labels())
                    .collect::<HashSet<String>>(),
            ),
            (
                "csync".to_string(),
                "".to_string(),
                hashset_strings![db
                    .get("github", "fioncat", "csync")
                    .unwrap()
                    .name_with_labels()],
            ),
            (
                "gitlab".to_string(),
                "02".to_string(),
                hashset_strings![db
                    .get("gitlab", "my-owner-01", "my-repo-02")
                    .unwrap()
                    .name_with_labels()],
            ),
        ];

        let opts = new_test_options(None, None);
        for case in cases {
            let (head, query, expect) = case;
            let selector = Selector::new(&head, &query, opts.clone());
            let (repos, _) = selector.many_local(&db).unwrap();
            let repos: HashSet<String> = repos.iter().map(|repo| repo.name_with_labels()).collect();

            assert_eq!(repos, expect);
        }
    }
}

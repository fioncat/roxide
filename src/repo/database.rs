#![allow(dead_code)]

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;
use std::{fs, io};

use anyhow::{bail, Context, Result};
use bincode::Options;
use serde::{Deserialize, Serialize};

use crate::config::{self, Config};
use crate::repo::{Owner, Remote, Repo};
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
    /// The bucket data structure undergoes some special optimizations—it, stores
    /// remote, owner, and label data only once, while repositories store indices
    /// of these data. Information about remote, owner, and label for a particular
    /// repository is located through these indices.
    ///
    /// # Arguments
    ///
    /// * `repos` - The repositories to convert, note that please ensure that the
    /// `Rc` references are unique when calling this method; otherwise, the function
    /// will panic.
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
    fn build_repos(self, cfg: &config::Config) -> Result<Vec<Rc<Repo>>> {
        let Bucket {
            remotes: remote_names,
            owners: owner_names,
            labels: all_label_names,
            repos: all_buckets,
        } = self;

        let mut remote_index: HashMap<u32, Rc<Remote>> = HashMap::with_capacity(remote_names.len());
        for (idx, remote_name) in remote_names.into_iter().enumerate() {
            let remote_cfg = match cfg.get_remote(&remote_name)  {
                Some(remote_cfg) => remote_cfg,
                None => bail!("could not find remote config for '{remote_name}', please add it to your config file"),
            };

            let remote = Rc::new(Remote {
                name: remote_name,
                cfg: remote_cfg,
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
                    let owner_cfg = cfg.get_owner(&remote.name, &owner_name);
                    let owner = Rc::new(Owner {
                        name: owner_name,
                        remote: owner_remote,
                        cfg: owner_cfg,
                    });
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
        let path = PathBuf::from(&cfg.metadir).join("database");
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
    /// # Arguements
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

#[cfg(test)]
mod bucket_tests {
    use crate::config::tests as config_tests;
    use crate::repo::database::*;
    use crate::{hash_set, vec_strings};

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
                    labels: Some(hash_set![0]),
                },
                RepoBucket {
                    remote: 0,
                    owner: 0,
                    name: String::from("spacenvim"),
                    accessed: 11.0,
                    last_accessed: 11,
                    path: None,
                    labels: Some(hash_set![0]),
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
                    labels: Some(hash_set![1]),
                },
            ],
        }
    }

    fn get_expect_repos(cfg: &config::Config) -> Vec<Rc<Repo>> {
        let github_remote = Rc::new(Remote {
            name: String::from("github"),
            cfg: cfg.get_remote("github").unwrap(),
        });
        let gitlab_remote = Rc::new(Remote {
            name: String::from("gitlab"),
            cfg: cfg.get_remote("gitlab").unwrap(),
        });
        let fioncat_owner = Rc::new(Owner {
            name: String::from("fioncat"),
            remote: Rc::clone(&github_remote),
            cfg: cfg.get_owner("github", "fioncat"),
        });
        let kubernetes_owner = Rc::new(Owner {
            name: String::from("kubernetes"),
            remote: Rc::clone(&github_remote),
            cfg: cfg.get_owner("github", "kubernetes"),
        });
        let test_owner = Rc::new(Owner {
            name: String::from("test"),
            remote: Rc::clone(&gitlab_remote),
            cfg: cfg.get_owner("gitlab", "test"),
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
                labels: Some(hash_set![Rc::clone(&pin_label)]),
            }),
            Rc::new(Repo {
                name: String::from("spacenvim"),
                remote: Rc::clone(&github_remote),
                owner: Rc::clone(&fioncat_owner),
                path: None,
                accessed: 11.0,
                last_accessed: 11,
                labels: Some(hash_set![pin_label]),
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
                labels: Some(hash_set![sync_label]),
            }),
        ]
    }

    #[test]
    fn test_bucket_from() {
        let cfg = config_tests::get_test_config("test_bucket_from");

        let expect_bucket = get_expect_bucket();
        let repos = get_expect_repos(&cfg);

        let bucket = Bucket::from_repos(repos);
        assert_eq!(expect_bucket, bucket);
    }

    #[test]
    fn test_bucket_build() {
        let cfg = config_tests::get_test_config("test_bucket_build");

        let expect_repos = get_expect_repos(&cfg);

        let bucket = get_expect_bucket();
        let repos = bucket.build_repos(&cfg).unwrap();

        assert_eq!(expect_repos, repos);
    }

    #[test]
    fn test_bucket_convert() {
        let cfg = config_tests::get_test_config("test_bucket_convert");

        let expect_repos = get_expect_repos(&cfg);
        let repos = get_expect_repos(&cfg);

        let bucket = Bucket::from_repos(repos);
        let repos = bucket.build_repos(&cfg).unwrap();

        assert_eq!(expect_repos, repos);
    }
}

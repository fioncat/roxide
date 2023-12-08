pub mod database;
pub mod snapshot;

use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::Rc;

use anyhow::Result;

use crate::api::ApiUpstream;
use crate::config::{Config, OwnerConfig, RemoteConfig};
use crate::utils;

/// Represents the remote to which the repository belongs, including the config
/// information of the remote.
#[derive(Debug, PartialEq, Clone)]
pub struct Remote {
    /// The remote name.
    pub name: String,

    /// The remote config.
    pub cfg: RemoteConfig,
}

/// Represents the owner to which the repository belongs, including the config of
/// the owner and the associated remote.
#[derive(Debug, PartialEq)]
pub struct Owner {
    /// The owner name.
    pub name: String,

    /// The remote to which this owner belongs.
    pub remote: Rc<Remote>,

    /// The owner config. If it is [`None`], it indicates that this owner does not
    /// have a config.
    pub cfg: Option<OwnerConfig>,
}

/// Represents a repository, which is the most fundamental operational object,
/// typically used in the form of [`Rc`].
#[derive(Debug, PartialEq)]
pub struct Repo {
    /// The repository name.
    pub name: String,

    /// The remote to which this repository belongs.
    pub remote: Rc<Remote>,
    /// The owner to which this repository belongs.
    pub owner: Rc<Owner>,

    /// The local path of the repository, optional. If it is [`None`], it means that
    /// the repository is located under the `workspace`, and the local path will be
    /// dynamically generated.
    pub path: Option<String>,

    /// The Unix timestamp of the last visit to this repository.
    pub last_accessed: u64,
    /// The number of times this repository has been accessed.
    pub accessed: f64,

    /// Labels for this repository. Labels are typically used for filtering and
    /// other operations.
    ///
    /// Some special labels may have specific effects in certain scenarios, such as
    /// `pin`, `sync`, etc. Refer to the documentation for individual commands for
    /// more details.
    pub labels: Option<HashSet<Rc<String>>>,
}

/// Represents how to display the repository, passes to [`Repo::to_string`].
pub enum NameLevel {
    /// Only display name.
    Name,
    /// Display owner and name: `{owner}/{name}`.
    Owner,
    /// Display remote, owner and name: `{remote}:{owner}/{name}`
    Remote,
}

impl Repo {
    /// Create a new repo [`Rc`] object.
    pub fn new<S, O, N>(
        cfg: &Config,
        remote_name: S,
        owner_name: O,
        name: N,
        path: Option<String>,
        labels: &Option<HashSet<String>>,
    ) -> Result<Rc<Repo>>
    where
        S: AsRef<str>,
        O: AsRef<str>,
        N: AsRef<str>,
    {
        // Get remote and owner config, create new `Rc` object.
        // Here all the configs will be cloned.
        let remote_cfg = cfg.must_get_remote(remote_name.as_ref())?;
        let remote = Rc::new(Remote {
            name: remote_name.as_ref().to_string(),
            cfg: remote_cfg.clone(),
        });
        let owner = match cfg.get_owner(remote_name.as_ref(), owner_name.as_ref()) {
            Some(owner_cfg) => Rc::new(Owner {
                name: owner_name.as_ref().to_string(),
                cfg: Some(owner_cfg.clone()),
                remote: Rc::clone(&remote),
            }),
            None => Rc::new(Owner {
                name: owner_name.as_ref().to_string(),
                cfg: None,
                remote: Rc::clone(&remote),
            }),
        };

        let name = name.as_ref().to_string();

        let mut labels: Option<HashSet<String>> = match labels {
            Some(labels) => Some(labels.iter().map(|label| label.clone()).collect()),
            None => None,
        };

        // If the config includes labels that do not currently exist in the
        // repository, we need to extend these labels to the repository.
        let mut append_cfg_labels = |cfg_labels: Option<HashSet<String>>| {
            if let Some(cfg_labels) = cfg_labels {
                match labels.as_mut() {
                    Some(labels) => labels.extend(cfg_labels),
                    None => labels = Some(cfg_labels),
                }
            }
        };
        append_cfg_labels(remote_cfg.labels.clone());
        if let Some(owner_cfg) = owner.cfg.as_ref() {
            append_cfg_labels(owner_cfg.labels.clone());
        }

        // Convert `String` in labels to `Rc<String>`
        let labels = match labels {
            Some(labels) => Some(labels.into_iter().map(|label| Rc::new(label)).collect()),
            None => None,
        };

        Ok(Rc::new(Repo {
            name,
            remote,
            owner,
            path,
            last_accessed: cfg.now(),
            accessed: 0.0,
            labels,
        }))
    }

    /// Use [`ApiUpstream`] to build a repository object.
    pub fn from_api_upstream<S>(
        cfg: &Config,
        remote_name: S,
        upstream: ApiUpstream,
    ) -> Result<Rc<Repo>>
    where
        S: AsRef<str>,
    {
        Self::new(
            cfg,
            remote_name.as_ref(),
            &upstream.owner,
            &upstream.name,
            None,
            &None,
        )
    }

    /// Retrieve the repository path.
    ///
    /// If the user explicitly specifies a path for the repository, clone it directly
    /// and return that path. If no path is specified, consider the repository to be
    /// in the workspace and generate a path using the rule:
    ///
    /// * `{workspace}/{remote}/{owner}/{name}`.
    pub fn get_path(&self, cfg: &Config) -> PathBuf {
        if let Some(path) = self.path.as_ref() {
            return PathBuf::from(path);
        }

        cfg.get_workspace_dir()
            .join(&self.remote.name)
            .join(&self.owner.name)
            .join(&self.name)
    }

    /// Check if the repository contains the specified labels.
    pub fn contains_labels(&self, labels: &Option<HashSet<String>>) -> bool {
        match labels {
            Some(labels) => {
                if labels.is_empty() {
                    return true;
                }
                if let None = self.labels {
                    return false;
                }
                let repo_labels = self.labels.as_ref().unwrap();
                for label in labels {
                    if !repo_labels.contains(label) {
                        return false;
                    }
                }
                true
            }
            None => true,
        }
    }

    /// Update the repository, which will update the following fields:
    ///
    /// * `last_accessed`: Updated to the current time.
    /// * `accessed`: Incremented by 1.
    /// * `labels`: Append if the `labels` parameter is not [`None`].
    ///
    /// This function will return a new repository [`Rc`], but the new object's
    /// internal references to remote and owner will be shared with the original.
    pub fn update(&self, cfg: &Config, append_labels: Option<HashSet<String>>) -> Rc<Repo> {
        let append_labels: Option<HashSet<Rc<String>>> = match append_labels {
            Some(labels) => Some(labels.into_iter().map(|label| Rc::new(label)).collect()),
            None => None,
        };

        let new_labels = match self.labels.as_ref() {
            Some(current_labels) => match append_labels {
                Some(append_labels) => {
                    let mut new_labels = current_labels.clone();
                    new_labels.extend(append_labels);
                    Some(new_labels)
                }
                None => Some(current_labels.clone()),
            },
            None => append_labels,
        };
        Rc::new(Repo {
            remote: Rc::clone(&self.remote),
            owner: Rc::clone(&self.owner),
            name: self.name.clone(),
            path: self.path.clone(),
            last_accessed: cfg.now(),
            accessed: self.accessed + 1.0,
            labels: new_labels,
        })
    }

    /// Update repo labels only, keep other info. Use [`Rc`] as labels type.
    pub fn update_labels_rc(&self, labels: Option<HashSet<Rc<String>>>) -> Rc<Repo> {
        Rc::new(Repo {
            remote: Rc::clone(&self.remote),
            owner: Rc::clone(&self.owner),
            name: self.name.clone(),
            path: self.path.clone(),
            last_accessed: self.last_accessed,
            accessed: self.accessed,
            labels,
        })
    }

    /// Update repo labels only, keep other info.
    pub fn update_labels(&self, labels: Option<HashSet<String>>) -> Rc<Repo> {
        let labels = match labels {
            Some(labels) => Some(labels.into_iter().map(|label| Rc::new(label)).collect()),
            None => None,
        };
        self.update_labels_rc(labels)
    }

    /// `score` is used to sort and prioritize multiple repositories. In scenarios
    /// like fuzzy matching, repositories with higher scores are matched first.
    ///
    /// The scoring algorithm here is borrowed from:
    ///
    /// * [zoxide](https://github.com/ajeetdsouza/zoxide)
    ///
    /// For details, please refer to:
    ///
    /// * [Algorithm](https://github.com/ajeetdsouza/zoxide/wiki/Algorithm#frecency)
    pub fn score(&self, cfg: &Config) -> f64 {
        let duration = cfg.now().saturating_sub(self.last_accessed);
        if duration < utils::HOUR {
            self.accessed * 4.0
        } else if duration < utils::DAY {
            self.accessed * 2.0
        } else if duration < utils::WEEK {
            self.accessed * 0.5
        } else {
            self.accessed * 0.25
        }
    }

    /// Retrieve the `git clone` URL for this repository.
    ///
    /// Alias rules from the configuration will be applied here.
    pub fn clone_url(&self) -> String {
        Self::get_clone_url(self.owner.name.as_str(), self.name.as_str(), &self.remote)
    }

    /// Retrieve the `git clone` URL for the specified repository.
    ///
    /// Alias rules from the configuration will be applied here.
    pub fn get_clone_url<O, N>(owner: O, name: N, remote: &Remote) -> String
    where
        O: AsRef<str>,
        N: AsRef<str>,
    {
        if remote.cfg.has_alias() {
            let owner = match remote.cfg.alias_owner(owner.as_ref()) {
                Some(name) => name,
                None => owner.as_ref(),
            };
            let name = match remote.cfg.alias_repo(owner, name.as_ref()) {
                Some(name) => name,
                None => name.as_ref(),
            };

            return Self::get_clone_url_without_alias(owner, name, remote);
        }
        Self::get_clone_url_without_alias(owner, name, remote)
    }

    /// Retrieve the `git clone` URL for the specified repository.
    ///
    /// The alias rules in config will be ignored.
    pub fn get_clone_url_without_alias<O, N>(owner: O, name: N, remote: &Remote) -> String
    where
        O: AsRef<str>,
        N: AsRef<str>,
    {
        let mut ssh = remote.cfg.ssh;
        if let Some(owner_cfg) = remote.cfg.owners.get(owner.as_ref()) {
            if let Some(use_ssh) = owner_cfg.ssh {
                ssh = use_ssh;
            }
        }

        let domain = match remote.cfg.clone.as_ref() {
            Some(domain) => domain.as_str(),
            None => "github.com",
        };

        if ssh {
            format!("git@{}:{}/{}.git", domain, owner.as_ref(), name.as_ref())
        } else {
            format!(
                "https://{}/{}/{}.git",
                domain,
                owner.as_ref(),
                name.as_ref()
            )
        }
    }

    /// Retrieve the workspace path for this repository.
    pub fn get_workspace_path<R, O, N>(cfg: &Config, remote: R, owner: O, name: N) -> PathBuf
    where
        R: AsRef<str>,
        O: AsRef<str>,
        N: AsRef<str>,
    {
        cfg.get_workspace_dir()
            .join(remote.as_ref())
            .join(owner.as_ref())
            .join(name.as_ref())
    }

    /// Build the string to display this repository. See: [`NameLevel`].
    pub fn to_string(&self, level: &NameLevel) -> String {
        match level {
            NameLevel::Name => self.name.clone(),
            NameLevel::Owner => self.name_with_owner(),
            NameLevel::Remote => self.name_with_remote(),
        }
    }

    /// Build the labels show string, return [`None`] if this repository has no label.
    pub fn labels_string(&self) -> Option<String> {
        match self.labels.as_ref() {
            Some(labels) => {
                let mut labels: Vec<String> =
                    labels.iter().map(|label| label.to_string()).collect();
                labels.sort();
                Some(labels.join(","))
            }
            None => None,
        }
    }

    /// Show repository with owner, See: [`NameLevel::Owner`].
    pub fn name_with_owner(&self) -> String {
        format!("{}/{}", self.owner.name, self.name)
    }

    /// Show repository with remote, See: [`NameLevel::Remote`].
    pub fn name_with_remote(&self) -> String {
        format!("{}:{}/{}", self.remote.name, self.owner.name, self.name)
    }

    /// Show repository with labels, See: [`NameLevel::Labels`].
    pub fn name_with_labels(&self) -> String {
        match self.labels_string() {
            Some(labels) => {
                format!(
                    "{}:{}/{}@{}",
                    self.remote.name, self.owner.name, self.name, labels
                )
            }
            None => self.name_with_remote(),
        }
    }
}

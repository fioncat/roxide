pub mod database;
pub mod detect;
pub mod keywords;
pub mod snapshot;

use std::collections::HashSet;
use std::{borrow::Cow, path::PathBuf};

use anyhow::Result;

use crate::api::ApiUpstream;
use crate::config::{defaults, Config, RemoteConfig};
use crate::utils;

/// Represents a repository, which is the most fundamental operational object
#[derive(Debug, Clone)]
pub struct Repo<'a> {
    /// The repository remote name.
    pub remote: Cow<'a, str>,
    /// The repository owner name.
    pub owner: Cow<'a, str>,
    /// The repository name.
    pub name: Cow<'a, str>,

    /// The local path of the repository, optional. If it is [`None`], it means that
    /// the repository is located under the `workspace`, and the local path will be
    /// dynamically generated.
    pub path: Option<Cow<'a, str>>,

    /// Labels for this repository. Labels are typically used for filtering and
    /// other operations.
    ///
    /// Some special labels may have specific effects in certain scenarios, such as
    /// `pin`, `sync`, etc. Refer to the documentation for individual commands for
    /// more details.
    pub labels: Option<HashSet<Cow<'a, str>>>,

    /// The Unix timestamp of the last visit to this repository.
    pub last_accessed: u64,
    /// The number of times this repository has been accessed.
    pub accessed: u64,

    /// The remote config reference.
    pub remote_cfg: Cow<'a, RemoteConfig>,
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

impl PartialEq for Repo<'_> {
    fn eq(&self, other: &Self) -> bool {
        if self.remote != other.remote {
            return false;
        }
        if self.owner != other.owner {
            return false;
        }
        if self.name != other.name {
            return false;
        }
        true
    }
}

impl Repo<'_> {
    #[inline]
    pub fn new<'a>(
        cfg: &'a Config,
        remote: Cow<'a, str>,
        owner: Cow<'a, str>,
        name: Cow<'a, str>,
        path: Option<String>,
    ) -> Result<Repo<'a>> {
        let remote_cfg = cfg.must_get_remote(remote.as_ref())?;
        let owner_cfg = cfg.get_owner(remote.as_ref(), owner.as_ref());

        let mut labels: Option<HashSet<Cow<'a, str>>> = remote_cfg.labels.as_ref().map(|labels| {
            labels
                .iter()
                .map(|label| Cow::Owned(label.to_string()))
                .collect()
        });
        if let Some(owner_cfg) = owner_cfg.as_ref() {
            if let Some(owner_labels) = owner_cfg.labels.as_ref() {
                let owner_labels = owner_labels
                    .iter()
                    .map(|label| Cow::Owned(label.clone()))
                    .collect();
                match labels.as_mut() {
                    Some(labels) => labels.extend(owner_labels),
                    None => labels = Some(owner_labels),
                }
            }
        }

        Ok(Repo {
            remote,
            owner,
            name,
            accessed: 0,
            last_accessed: 0,
            labels,
            remote_cfg,
            path: path.map(Cow::Owned),
        })
    }

    #[inline]
    pub fn update<'a>(self) -> Repo<'a> {
        Repo {
            remote: Cow::Owned(self.remote.to_string()),
            owner: Cow::Owned(self.owner.to_string()),
            name: Cow::Owned(self.name.to_string()),
            path: self.path.map(|path| Cow::Owned(path.to_string())),
            labels: self.labels.map(|labels| {
                labels
                    .into_iter()
                    .map(|label| Cow::Owned(label.to_string()))
                    .collect()
            }),
            last_accessed: self.last_accessed,
            accessed: self.accessed,
            remote_cfg: Cow::Owned(defaults::remote("")),
        }
    }

    /// Use [`ApiUpstream`] to build a repository object.
    #[inline]
    pub fn from_api_upstream(cfg: &Config, remote: impl AsRef<str>, upstream: ApiUpstream) -> Repo {
        Repo {
            remote: Cow::Owned(remote.as_ref().to_string()),
            owner: Cow::Owned(upstream.owner),
            name: Cow::Owned(upstream.name),
            accessed: 0,
            last_accessed: 0,
            labels: None,
            remote_cfg: cfg.get_remote_or_default(remote),
            path: None,
        }
    }

    /// Retrieve the repository path.
    ///
    /// If the user explicitly specifies a path for the repository, clone it directly
    /// and return that path. If no path is specified, consider the repository to be
    /// in the workspace and generate a path using the rule:
    ///
    /// * `{workspace}/{remote}/{owner}/{name}`.
    pub fn get_path(&self, cfg: &Config) -> PathBuf {
        database::get_path(
            cfg,
            &self.path,
            self.remote.as_ref(),
            self.owner.as_ref(),
            self.name.as_ref(),
        )
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
    pub fn score(&self, cfg: &Config) -> u64 {
        let duration = cfg.now().saturating_sub(self.last_accessed);
        if duration < utils::HOUR {
            self.accessed * 16
        } else if duration < utils::DAY {
            self.accessed * 8
        } else if duration < utils::WEEK {
            self.accessed * 2
        } else {
            self.accessed
        }
    }

    /// Build the string to display this repository. See: [`NameLevel`].
    pub fn to_string(&self, level: &NameLevel) -> String {
        match level {
            NameLevel::Name => self.name.to_string(),
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
        format!("{}/{}", self.owner, self.name)
    }

    /// Show repository with remote, See: [`NameLevel::Remote`].
    pub fn name_with_remote(&self) -> String {
        format!("{}:{}/{}", self.remote, self.owner, self.name)
    }

    /// Show repository with labels, See: [`NameLevel::Labels`].
    #[cfg(test)]
    pub fn name_with_labels(&self) -> String {
        match self.labels_string() {
            Some(labels) => {
                format!("{}:{}/{}@{}", self.remote, self.owner, self.name, labels)
            }
            None => self.name_with_remote(),
        }
    }

    /// Retrieve the `git clone` URL for this repository.
    ///
    /// Alias rules from the configuration will be applied here.
    pub fn clone_url(&self) -> String {
        Self::get_clone_url(&self.owner, &self.name, self.remote_cfg.as_ref())
    }

    /// Retrieve the `git clone` URL for the specified repository.
    ///
    /// Alias rules from the configuration will be applied here.
    pub fn get_clone_url<O, N>(owner: O, name: N, remote_cfg: &RemoteConfig) -> String
    where
        O: AsRef<str>,
        N: AsRef<str>,
    {
        if remote_cfg.has_alias() {
            let owner = remote_cfg
                .alias_owner(owner.as_ref())
                .unwrap_or_else(|| owner.as_ref());
            let name = remote_cfg
                .alias_repo(owner, name.as_ref())
                .unwrap_or_else(|| name.as_ref());

            return Self::get_clone_url_without_alias(owner, name, remote_cfg);
        }
        Self::get_clone_url_without_alias(owner, name, remote_cfg)
    }

    /// Retrieve the `git clone` URL for the specified repository.
    ///
    /// The alias rules in config will be ignored.
    pub fn get_clone_url_without_alias<O, N>(owner: O, name: N, remote_cfg: &RemoteConfig) -> String
    where
        O: AsRef<str>,
        N: AsRef<str>,
    {
        let mut ssh = remote_cfg.ssh;
        if let Some(owner_cfg) = remote_cfg.owners.get(owner.as_ref()) {
            if let Some(use_ssh) = owner_cfg.ssh {
                ssh = use_ssh;
            }
        }

        let domain = match remote_cfg.clone.as_ref() {
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

    pub fn append_labels(&mut self, labels: Option<HashSet<String>>) {
        if labels.is_none() {
            return;
        }
        let append_labels: HashSet<_> = labels.unwrap().into_iter().map(Cow::Owned).collect();
        if let Some(labels) = self.labels.as_mut() {
            labels.extend(append_labels);
        } else {
            self.labels = Some(append_labels);
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
}

pub mod database;

use std::collections::HashSet;
use std::{borrow::Cow, path::PathBuf};

use anyhow::Result;

use crate::api::ApiUpstream;
use crate::config::{Config, OwnerConfig, RemoteConfig};

#[derive(Debug)]
pub struct Repo<'a> {
    pub remote: Cow<'a, str>,
    pub owner: Cow<'a, str>,
    pub name: Cow<'a, str>,

    pub path: Option<Cow<'a, str>>,

    pub labels: Option<HashSet<Cow<'a, str>>>,

    pub last_accessed: u64,
    pub accessed: f64,

    pub remote_cfg: Option<Cow<'a, RemoteConfig>>,
    pub owner_cfg: Option<Cow<'a, OwnerConfig>>,
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

impl Repo<'_> {
    /// Use [`ApiUpstream`] to build a repository object.
    pub fn from_api_upstream(cfg: &Config, remote: impl AsRef<str>, upstream: ApiUpstream) -> Repo {
        Repo {
            remote: Cow::Owned(remote.as_ref().to_string()),
            owner: Cow::Owned(upstream.owner),
            name: Cow::Owned(upstream.name),
            accessed: 0.0,
            last_accessed: 0,
            labels: None,
            remote_cfg: cfg.get_remote(remote.as_ref()),
            owner_cfg: cfg.get_owner(remote.as_ref(), &upstream.owner),
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

    pub fn score(&self, cfg: &Config) -> u64 {
        todo!()
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
        Self::get_clone_url(self.owner, self.name, self.remote_cfg.as_ref().unwrap())
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
            let owner = match remote_cfg.alias_owner(owner.as_ref()) {
                Some(name) => name,
                None => owner.as_ref(),
            };
            let name = match remote_cfg.alias_repo(owner, name.as_ref()) {
                Some(name) => name,
                None => name.as_ref(),
            };

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
}

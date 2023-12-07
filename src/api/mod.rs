#![allow(dead_code)]

mod alias;
mod cache;
pub mod github;
mod gitlab;

use std::time::Duration;

use anyhow::{bail, Result};
use console::style;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};

use crate::api::alias::Alias;
use crate::api::cache::Cache;
use crate::api::github::Github;
use crate::api::gitlab::Gitlab;
use crate::config::{Config, ProviderType};
use crate::repo::Remote;

/// Represents repository information obtained from a [`Provider`].
#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub struct ApiRepo {
    /// The default branch for this repository.
    pub default_branch: String,

    /// If this repository is a fork from another repository, it represents the
    /// fork source of this repository.
    ///
    /// If the repository is not a fork, it will be [`None`].
    pub upstream: Option<ApiUpstream>,

    /// The web access URL for this repository. Typically, open it in a web browser.
    pub web_url: String,
}

/// Information about the fork source of the repository.
#[derive(Debug, PartialEq, Deserialize, Serialize, Clone)]
pub struct ApiUpstream {
    /// The fork source owner.
    pub owner: String,
    /// The fork source name.
    pub name: String,
    /// The fork source default branch name.
    pub default_branch: String,
}

impl ApiUpstream {
    /// Convert fork source to string, the format is '{owner}/{name}'.
    pub fn to_string(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }
}

/// Represents the information needed to create or retrieve a MergeRequest
/// (PullRequest in GitHub).
#[derive(Debug, Clone)]
pub struct MergeOptions {
    /// The repository owner.
    pub owner: String,
    /// The repository name.
    pub name: String,

    /// If not [`None`], it indicates that this merge needs to be merged into
    /// the upstream.
    pub upstream: Option<ApiUpstream>,

    /// The source branch name.
    pub source: String,
    /// The target branch name.
    pub target: String,
}

impl MergeOptions {
    /// Display merge options with terminal color.
    pub fn pretty_display(&self) -> String {
        match &self.upstream {
            Some(upstream) => format!(
                "{}:{} => {}:{}",
                style(self.to_string()).yellow(),
                style(&self.source).magenta(),
                style(upstream.to_string()).yellow(),
                style(&self.target).magenta()
            ),
            None => format!(
                "{} => {}",
                style(&self.source).magenta(),
                style(&self.target).magenta()
            ),
        }
    }

    /// Convert merge options to string, the format is '{owner}/{name}'.
    pub fn to_string(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }
}

/// A `Provider` is an API abstraction for a remote, providing functions for
/// interacting with remote repository storage.
///
/// Behind a `Provider` could be the GitHub or GitLab API, or it could include a
/// cache layer. External components do not need to be concerned with the internal
/// implementation of the provider.
pub trait Provider {
    /// Retrieve all repositories under a given owner.
    fn list_repos(&self, owner: &str) -> Result<Vec<String>>;

    /// Retrieve information for a specific repository.
    fn get_repo(&self, owner: &str, name: &str) -> Result<ApiRepo>;

    /// Attempt to retrieve a MergeRequest (PullRequest in Github) and return the
    /// URL of that MergeRequest.
    ///
    /// If the Merge Request does not exist, return `Ok(None)`.
    fn get_merge(&self, merge: MergeOptions) -> Result<Option<String>>;

    /// Create MergeRequest (PullRequest in Github), and return its URL.
    fn create_merge(&mut self, merge: MergeOptions, title: String, body: String) -> Result<String>;

    /// Search repositories using the specified `query`.
    fn search_repos(&self, query: &str) -> Result<Vec<String>>;
}

/// Build common http client.
fn build_common_client(remote: &Remote) -> Client {
    Client::builder()
        .timeout(Duration::from_secs(remote.cfg.api_timeout))
        .build()
        .unwrap()
}

/// Build a [`Provider`] object based on the configuration.
///
/// # Arguments
///
/// * `cfg` - We need configuration about `cache`, `limit`, and other settings from
/// the [`Config`] to build the [`Provider`]. Note that if `cache` is greater
/// than 0, a caching layer will be added on top of the original [`Provider`] to
/// speed up calls. All cached data will be stored in `{metadir}/cache`.
/// * `remote` - If the provider type is not configured in the remote, the build
/// will fail. The [`Provider`] will use the token from the remote for authentication.
/// Additionally, if the remote has aliases configured, an alias layer will be added
/// on top of the original [`Provider`] to convert repository alias names to real
/// names so that the remote API can correctly identify them.
/// * `force` - Only effective when cache is enabled, indicating that the current
/// cache should be forcibly expired to refresh cache data.
pub fn build_provider(cfg: &Config, remote: &Remote, force: bool) -> Result<Box<dyn Provider>> {
    if let None = remote.cfg.provider {
        bail!("missing provider config for remote '{}'", remote.name);
    }

    let mut provider = match remote.cfg.provider.as_ref().unwrap() {
        ProviderType::Github => Github::new(remote),
        ProviderType::Gitlab => Gitlab::new(remote),
    };

    if remote.cfg.cache_hours > 0 {
        let cache = Cache::new(cfg, remote, provider, force)?;
        provider = Box::new(cache);
    }

    if remote.cfg.has_alias() {
        let (alias_owner, alias_repo) = remote.cfg.get_alias_map();
        provider = Alias::new(alias_owner, alias_repo, provider);
    }

    Ok(provider)
}

#[cfg(test)]
pub mod api_tests {
    use std::collections::{HashMap, HashSet};

    use anyhow::bail;

    use crate::api::*;

    pub struct StaticProvider {
        repos: HashMap<String, Vec<String>>,

        merges: HashSet<String>,
    }

    impl StaticProvider {
        pub fn new(repos: Vec<(&str, Vec<&str>)>) -> Box<dyn Provider> {
            let p = StaticProvider {
                repos: repos
                    .into_iter()
                    .map(|(key, value)| {
                        (
                            key.to_string(),
                            value.into_iter().map(|value| value.to_string()).collect(),
                        )
                    })
                    .collect(),
                merges: HashSet::new(),
            };
            Box::new(p)
        }

        pub fn mock() -> Box<dyn Provider> {
            Self::new(vec![
                (
                    "fioncat",
                    vec!["roxide", "spacenvim", "dotfiles", "fioncat"],
                ),
                (
                    "kubernetes",
                    vec!["kubernetes", "kube-proxy", "kubelet", "kubectl"],
                ),
            ])
        }
    }

    impl Provider for StaticProvider {
        fn list_repos(&self, owner: &str) -> Result<Vec<String>> {
            match self.repos.get(owner) {
                Some(repos) => Ok(repos.clone()),
                None => bail!("could not find owner {owner}"),
            }
        }

        fn get_repo(&self, owner: &str, name: &str) -> Result<ApiRepo> {
            match self.repos.get(owner) {
                Some(repos) => match repos.iter().find_map(|repo_name| {
                    if repo_name == name {
                        Some(ApiRepo {
                            default_branch: String::from("main"),
                            upstream: None,
                            web_url: String::new(),
                        })
                    } else {
                        None
                    }
                }) {
                    Some(repo) => Ok(repo),
                    None => bail!("Could not find repo {name} under {owner}"),
                },
                None => bail!("Could not find owner {owner}"),
            }
        }

        fn get_merge(&self, merge: MergeOptions) -> Result<Option<String>> {
            self.get_repo(&merge.owner, &merge.name)?;
            match self.merges.get(merge.to_string().as_str()) {
                Some(merge) => Ok(Some(merge.clone())),
                None => Ok(None),
            }
        }

        fn create_merge(&mut self, merge: MergeOptions, _: String, _: String) -> Result<String> {
            self.get_repo(&merge.owner, &merge.name)?;
            let merge = merge.to_string();
            self.merges.insert(merge.clone());
            Ok(merge)
        }

        fn search_repos(&self, _query: &str) -> Result<Vec<String>> {
            Ok(Vec::new())
        }
    }
}

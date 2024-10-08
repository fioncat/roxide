mod alias;
mod cache;
pub mod github;
mod gitlab;

use std::fmt::Display;
use std::io::Write;
use std::time::Duration;

use anyhow::{bail, Result};
use console::style;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};

use crate::api::alias::Alias;
use crate::api::cache::Cache;
use crate::api::github::GitHub;
use crate::api::gitlab::GitLab;
use crate::config::{Config, ProviderType, RemoteConfig};

#[derive(Debug, Serialize)]
pub struct ProviderInfo {
    pub name: String,
    pub auth: bool,
    pub ping: bool,
}

impl Display for ProviderInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let auth = if self.auth { "with auth" } else { "no auth" };
        let ping = if self.ping {
            format!("ping {}", style("ok").green())
        } else {
            format!("ping {}", style("failed").red())
        };
        write!(f, "{}, {auth}, {ping}", self.name)
    }
}

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

impl Display for ApiUpstream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.owner, self.name)
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

impl Display for MergeOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.owner, self.name)
    }
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
}

/// The options to fetch [`Action`] from remote.
#[derive(Debug)]
pub struct ActionOptions {
    pub owner: String,
    pub name: String,

    pub target: ActionTarget,
}

/// ActionTarget defines how to locate the [`Action`]. By commit or branch.
#[derive(Debug)]
pub enum ActionTarget {
    /// Use commit SHA256 ID to locate the [`Action`].
    Commit(String),

    /// Use branch name to locate the [`Action`].
    Branch(String),
}

/// Action represents an abstraction of CI/CD. It represents all CI/CD tasks for a
/// particular commit.
///
/// In GitHub, it represents multiple `Action Runs`; In GitLab, it represents a single
/// `Pipeline`.
#[derive(Debug)]
pub struct Action {
    /// The action url. For GitHub, this is [`None`]; for GitLab, this is the url
    /// of `Pipeline`.
    pub url: Option<String>,

    /// The commit info for this action.
    pub commit: ActionCommit,

    /// All the action runs.
    pub runs: Vec<ActionRun>,
}

/// The commit info for an [`Action`].
#[derive(Debug)]
pub struct ActionCommit {
    /// The commit SHA256 ID.
    pub id: String,

    /// The commit title.
    pub message: String,

    /// The commit author username.
    pub author_name: String,

    /// The commit author email.
    pub author_email: String,
}

/// ActionRun represents a group of [`ActionJob`]. [`ActionJob`] is the entity that
/// actually performs the work, while ActionRun is responsible for categorizing them
/// for display purposes.
///
/// In GitHub, ActionRun is the `Action Run` resource; while in GitLab, ActionRun is
/// a `Stage` within a `Pipeline`.
#[derive(Debug)]
pub struct ActionRun {
    /// The name of the action run.
    pub name: String,

    /// For GitHub, this is the url of the action run; For GitLab, this is [`None`]
    /// (GitLab stage has no independent url).
    pub url: Option<String>,

    /// The set of jobs for this action run.
    pub jobs: Vec<ActionJob>,
}

/// The real CI/CD worker.
#[derive(Debug)]
pub struct ActionJob {
    /// The unique ID.
    pub id: u64,

    /// The job name.
    pub name: String,

    /// The job status.
    pub status: ActionJobStatus,

    /// The job url.
    pub url: String,
}

/// The CI/CD job status.
#[derive(Debug, PartialEq, Copy, Clone)]
pub enum ActionJobStatus {
    Pending,
    Running,

    Success,
    Failed,

    Canceled,
    Skipped,

    WaitingForConfirm,
}

impl Display for Action {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let id = if self.commit.id.len() > 8 {
            &self.commit.id[..8]
        } else {
            self.commit.id.as_str()
        };
        let message = self.commit.message.trim();
        writeln!(f, "Commit [{id}] {}", style(message).yellow(),)?;

        let author = format!("{} <{}>", self.commit.author_name, self.commit.author_email);
        write!(f, "Author {}", style(author).blue())?;

        Ok(())
    }
}

impl Display for ActionJobStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let msg = match self {
            Self::Pending => style("pending").yellow(),
            Self::Running => style("running").cyan(),
            Self::Success => style("success").green(),
            Self::Failed => style("failed").red(),
            Self::Canceled => style("canceled").yellow(),
            Self::Skipped => style("skipped").yellow(),
            Self::WaitingForConfirm => style("waiting_for_confirm").magenta(),
        };
        write!(f, "{msg}")
    }
}

impl ActionJobStatus {
    pub fn is_completed(&self) -> bool {
        !matches!(
            self,
            Self::Pending | Self::Running | Self::WaitingForConfirm
        )
    }
}

/// A `Provider` is an API abstraction for a remote, providing functions for
/// interacting with remote repository storage.
///
/// Behind a `Provider` could be the GitHub or GitLab API, or it could include a
/// cache layer. External components do not need to be concerned with the internal
/// implementation of the provider.
pub trait Provider {
    fn info(&self) -> Result<ProviderInfo>;

    /// Retrieve all repositories under a given owner.
    fn list_repos(&self, owner: &str) -> Result<Vec<String>>;

    /// Retrieve information for a specific repository.
    fn get_repo(&self, owner: &str, name: &str) -> Result<ApiRepo>;

    /// Attempt to retrieve a MergeRequest (PullRequest in Github) and return the
    /// URL of that MergeRequest.
    ///
    /// If the Merge Request does not exist, return `Ok(None)`.
    fn get_merge(&self, merge: MergeOptions) -> Result<Option<String>>;

    /// Create MergeRequest (PullRequest in GitHub), and return its URL.
    fn create_merge(&mut self, merge: MergeOptions, title: String, body: String) -> Result<String>;

    /// Search repositories using the specified `query`.
    fn search_repos(&self, query: &str) -> Result<Vec<String>>;

    /// Return the CI/CD action.
    fn get_action(&self, opts: &ActionOptions) -> Result<Option<Action>>;

    /// Get the logs of CI/CD a specific job.
    fn logs_job(&self, owner: &str, name: &str, id: u64, dst: &mut dyn Write) -> Result<()>;

    /// Get the info of a specific CI/CD job based on its ID.
    fn get_job(&self, owner: &str, name: &str, id: u64) -> Result<ActionJob>;
}

/// Build common http client.
fn build_common_client(remote_cfg: &RemoteConfig) -> Client {
    Client::builder()
        .timeout(Duration::from_secs(remote_cfg.api_timeout))
        .build()
        .unwrap()
}

/// Build a [`Provider`] object based on the configuration.
///
/// # Arguments
///
/// * `cfg` - We need configuration about `cache`, `limit`, and other settings from
///   the [`Config`] to build the [`Provider`]. Note that if `cache` is greater
///   than 0, a caching layer will be added on top of the original [`Provider`] to
///   speed up calls. All cached data will be stored in `{metadir}/cache`.
/// * `remote` - If the provider type is not configured in the remote, the build
///   will fail. The [`Provider`] will use the token from the remote for authentication.
///   Additionally, if the remote has aliases configured, an alias layer will be added
///   on top of the original [`Provider`] to convert repository alias names to real
///   names so that the remote API can correctly identify them.
/// * `force` - Only effective when cache is enabled, indicating that the current
///   cache should be forcibly expired to refresh cache data.
pub fn build_provider(
    cfg: &Config,
    remote_cfg: &RemoteConfig,
    force: bool,
) -> Result<Box<dyn Provider>> {
    if remote_cfg.provider.is_none() {
        bail!(
            "missing provider config for remote '{}'",
            remote_cfg.get_name()
        );
    }

    let mut provider = build_raw_provider(remote_cfg);

    if remote_cfg.cache_hours > 0 {
        let cache = Cache::new(cfg, remote_cfg, provider, force)?;
        provider = Box::new(cache);
    }

    if remote_cfg.has_alias() {
        let (alias_owner, alias_repo) = remote_cfg.get_alias_map();
        provider = Alias::build(alias_owner, alias_repo, provider);
    }

    Ok(provider)
}

pub fn build_raw_provider(remote_cfg: &RemoteConfig) -> Box<dyn Provider> {
    match remote_cfg.provider.as_ref().unwrap() {
        ProviderType::Github => GitHub::build(remote_cfg),
        ProviderType::Gitlab => GitLab::build(remote_cfg),
    }
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
        pub fn build(repos: Vec<(&str, Vec<&str>)>) -> Box<dyn Provider> {
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
            Self::build(vec![
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
        fn info(&self) -> Result<ProviderInfo> {
            todo!()
        }

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

        fn get_action(&self, _opts: &ActionOptions) -> Result<Option<Action>> {
            Ok(None)
        }

        fn logs_job(
            &self,
            _owner: &str,
            _name: &str,
            _id: u64,
            _dst: &mut dyn Write,
        ) -> Result<()> {
            Ok(())
        }

        fn get_job(&self, _owner: &str, _name: &str, _id: u64) -> Result<ActionJob> {
            todo!()
        }
    }
}

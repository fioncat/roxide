pub mod defaults;

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use serde::Deserialize;

/// The basic configuration, defining some global behaviors of roxide.
#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    /// The working directory, where all repo will be stored.
    #[serde(default = "defaults::workspace")]
    pub workspace: String,

    /// Store some meta data of repo, including database, cache, etc.
    #[serde(default = "defaults::metadir")]
    pub metadir: String,

    /// The generated command name, to support terminal navigation.
    #[serde(default = "defaults::cmd")]
    pub cmd: String,

    #[serde(default = "defaults::docker")]
    pub docker: Docker,

    /// The remotes config.
    #[serde(default = "defaults::empty_map")]
    pub remotes: HashMap<String, RemoteConfig>,

    /// The tag release rule.
    #[serde(default = "defaults::release")]
    pub release: HashMap<String, String>,

    /// Workflow can execute some pre-defined scripts on the repo.
    #[serde(default = "defaults::empty_map")]
    pub workflows: HashMap<String, WorkflowConfig>,

    #[serde(skip)]
    current_dir: Option<PathBuf>,

    #[serde(skip)]
    now: Option<u64>,

    #[serde(skip)]
    workspace_path: Option<PathBuf>,

    #[serde(skip)]
    meta_path: Option<PathBuf>,
}

#[derive(Debug, Deserialize, PartialEq, Clone)]
pub struct Docker {
    #[serde(default = "defaults::docker_cmd")]
    pub cmd: String,

    #[serde(default = "defaults::docker_shell")]
    pub shell: String,
}

#[derive(Debug, Deserialize, PartialEq, Clone)]
pub struct WorkflowConfig {
    #[serde(default = "defaults::empty_vec")]
    pub env: Vec<WorkflowEnv>,

    #[serde(default = "defaults::empty_vec")]
    pub steps: Vec<WorkflowStep>,
}

#[derive(Debug, Deserialize, PartialEq, Clone)]
pub struct WorkflowEnv {
    pub name: String,

    pub value: Option<String>,

    pub from_repo: Option<WorkflowFromRepo>,
}

#[derive(Debug, Deserialize, PartialEq, Clone)]
pub enum WorkflowFromRepo {
    #[serde(rename = "name")]
    Name,
    #[serde(rename = "owner")]
    Owner,
    #[serde(rename = "remote")]
    Remote,
    #[serde(rename = "path")]
    Path,
    #[serde(rename = "clone")]
    Clone,
}

/// Indicates an execution step in Workflow, which can be writing a file or
/// executing a shell command.
#[derive(Debug, Deserialize, PartialEq, Clone)]
pub struct WorkflowStep {
    /// If the Step is to write to a file, then name is the file name. If Step
    /// is an execution command, name is the name of the step. (required)
    pub name: String,

    pub image: Option<String>,

    pub ssh: Option<String>,

    pub work_dir: Option<String>,

    #[serde(default = "defaults::empty_vec")]
    pub env: Vec<WorkflowEnv>,

    /// If not empty, it is the file content.
    pub file: Option<String>,

    /// If not empty, it is the command to execute.
    pub run: Option<String>,

    pub capture_output: Option<String>,
}

/// RemoteConfig is a Git remote repository. Typical examples are Github and Gitlab.
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct RemoteConfig {
    /// The clone domain, for Github, is "github.com". If your remote Git repository
    /// is self-built, this is a private domain, such as "git.my.domain.com".
    ///
    /// If clone is not empty, all repo will be added to the workspace in the
    /// way of `git clone`. You need to create repo in the remote.
    /// If clone is empty, it means the remote is local. Repo is created locally
    /// via `mkdir`.
    ///
    /// The format of the clone url is:
    /// - https: `https://{clone_domain}/{repo_owner}/{repo_name}.git`
    /// - ssh: `git@{clone_domain}:{repo_owner}/{repo_name}.git`
    pub clone: Option<String>,

    /// User name, optional, if not empty, will execute the following command for
    /// each repo: `git config user.name {name}`
    pub user: Option<String>,

    /// User email, optional, if not empty, will execute the following command for
    /// each repo: `git config user.email {name}`
    pub email: Option<String>,

    /// If true, will use ssh protocol to clone repo, else, use https.
    #[serde(default = "defaults::disable")]
    pub ssh: bool,

    /// For new or cloned repositories, add the following labels.
    pub labels: Option<HashSet<String>>,

    /// The remote provider, If not empty, roxide will use remote api to enhance
    /// some capabilities, such as searching repos from remote.
    ///
    /// Currently only `github` and `gitlab` are supported. If your remote is of
    /// these two types, it is strongly recommended to enable it, and it is
    /// recommended to use it with `token` to complete the authentication.
    pub provider: Option<ProviderType>,

    /// Uses with `provider` to authenticate when calling api.
    ///
    /// For Github, see: https://docs.github.com/en/rest/overview/authenticating-to-the-rest-api?apiVersion=2022-11-28#authenticating-with-a-personal-access-token
    ///
    /// For Gitlab, see: https://docs.gitlab.com/ee/user/profile/personal_access_tokens.html
    ///
    /// You can fill in environment variables here, and they will be expanded when
    /// used.
    pub token: Option<String>,

    /// In order to speed up the response, after accessing the remote api, the
    /// data will be cached, which indicates the cache expiration time, in hours.
    /// Cache data will be stored under `{metadir}/cache`.
    ///
    /// If you wish to disable caching, set this value to 0.
    ///
    /// You can also add the `-f` parameter when executing the command to force
    /// roxide not to use the cache. This is useful if you know for sure that the
    /// remote information has been updated.
    #[serde(default = "defaults::cache_hours")]
    pub cache_hours: u32,

    /// The list limit when perform searching.
    #[serde(default = "defaults::list_limit")]
    pub list_limit: u32,

    /// The timeout seconds when requesting remote api.
    #[serde(default = "defaults::api_timeout")]
    pub api_timeout: u64,

    /// API domain, only useful for Gitlab. If your Git remote is self-built, it
    /// should be set to your self-built domain host.
    pub api_domain: Option<String>,

    /// Some personalized configurations for different owners.
    #[serde(default = "defaults::empty_map")]
    pub owners: HashMap<String, OwnerConfig>,

    #[serde(skip)]
    alias_owner_map: Option<HashMap<String, String>>,

    #[serde(skip)]
    alias_repo_map: Option<HashMap<String, HashMap<String, String>>>,
}

/// Owner configuration. Some configurations will override remote's.
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct OwnerConfig {
    /// Alias the remote owner to another name.
    pub alias: Option<String>,

    /// For new or cloned repositories, add the following labels.
    pub labels: Option<HashSet<String>>,

    /// Alias the remote repository to other names.
    #[serde(default = "defaults::empty_map")]
    pub repo_alias: HashMap<String, String>,

    /// If not empty, override remote's ssh.
    pub ssh: Option<bool>,

    /// After cloning or creating a repo, perform some additional workflows.
    pub on_create: Option<Vec<String>>,
}

/// The remote api provider type.
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub enum ProviderType {
    #[serde(rename = "github")]
    Github,
    #[serde(rename = "gitlab")]
    Gitlab,
}

impl Config {
    pub fn get_remote<'a>(&'a self, remote: impl AsRef<str>) -> Option<Cow<'a, RemoteConfig>> {
        let remote_cfg = self.remotes.get(remote.as_ref())?;
        Some(Cow::Borrowed(remote_cfg))
    }

    pub fn get_owner<'a, R, O>(&'a self, remote: R, owner: O) -> Option<Cow<'a, OwnerConfig>>
    where
        R: AsRef<str>,
        O: AsRef<str>,
    {
        let remote_cfg = self.remotes.get(remote.as_ref())?;
        let owner_cfg = remote_cfg.owners.get(owner.as_ref())?;
        Some(Cow::Borrowed(owner_cfg))
    }

    pub fn get_workspace_dir(&self) -> &PathBuf {
        self.workspace_path.as_ref().unwrap()
    }

    pub fn get_meta_dir(&self) -> &PathBuf {
        self.meta_path.as_ref().unwrap()
    }

    pub fn get_current_dir(&self) -> &PathBuf {
        self.current_dir.as_ref().unwrap()
    }
}

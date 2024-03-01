pub mod defaults;

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::SystemTime;
use std::{env, fs, io};

use anyhow::{bail, Context, Result};
use serde::Deserialize;

use crate::utils;

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

    #[serde(default = "defaults::keyword_expire")]
    pub keyword_expire: u64,

    /// The remotes' config.
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
    #[serde(default = "defaults::docker_name")]
    pub name: String,

    #[serde(default = "defaults::empty_vec")]
    pub args: Vec<String>,

    #[serde(default = "defaults::docker_shell")]
    pub shell: String,
}

#[derive(Debug, Deserialize, PartialEq, Clone)]
pub struct WorkflowConfig {
    #[serde(default = "defaults::empty_vec")]
    pub env: Vec<WorkflowEnv>,

    #[serde(default = "defaults::empty_vec")]
    pub steps: Vec<WorkflowStep>,

    #[serde(default = "defaults::empty_vec")]
    pub include: Vec<String>,
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

    pub os: Option<WorkflowOS>,

    #[serde(default = "defaults::empty_vec")]
    pub condition: Vec<WorkflowCondition>,

    #[serde(default = "defaults::disable")]
    pub allow_failure: bool,

    #[serde(rename = "if")]
    pub if_condition: Option<String>,

    pub image: Option<String>,

    pub ssh: Option<String>,

    pub set_env: Option<WorkflowSetEnv>,

    pub docker_build: Option<WorkflowDockerBuild>,

    pub docker_push: Option<String>,

    #[serde(default = "defaults::step_work_dir")]
    pub work_dir: String,

    #[serde(default = "defaults::empty_vec")]
    pub env: Vec<WorkflowEnv>,

    /// If not empty, it is the file content.
    pub file: Option<String>,

    /// If not empty, it is the command to execute.
    pub run: Option<String>,

    pub capture_output: Option<String>,
}

#[derive(Debug, Deserialize, PartialEq, Clone)]
pub struct WorkflowSetEnv {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Deserialize, PartialEq, Clone)]
pub struct WorkflowDockerBuild {
    pub image: String,

    #[serde(default = "defaults::docker_file")]
    pub file: String,
}

#[derive(Debug, Deserialize, PartialEq, Clone)]
pub enum WorkflowOS {
    #[serde(rename = "linux")]
    Linux,
    #[serde(rename = "macos")]
    Macos,
}

#[derive(Debug, Deserialize, PartialEq, Clone)]
pub struct WorkflowCondition {
    pub env: Option<String>,

    pub file: Option<String>,

    pub cmd: Option<String>,

    #[serde(default = "defaults::enable")]
    pub exists: bool,
}

/// RemoteConfig is a Git remote repository. Typical examples are GitHub and Gitlab.
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct RemoteConfig {
    /// The clone domain, for GitHub, is "github.com". If your remote Git repository
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

    /// Username, optional, if not empty, will execute the following command for
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
    /// Currently only `GitHub` and `GitLab` are supported. If your remote is of
    /// these two types, it is strongly recommended to enable it, and it is
    /// recommended to use it with `token` to complete the authentication.
    pub provider: Option<ProviderType>,

    /// Uses with `provider` to authenticate when calling api.
    ///
    /// For GitHub, see: https://docs.github.com/en/rest/overview/authenticating-to-the-rest-api?apiVersion=2022-11-28#authenticating-with-a-personal-access-token
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
    name: Option<String>,

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

impl RemoteConfig {
    pub fn get_name(&self) -> &str {
        self.name.as_ref().unwrap().as_str()
    }

    pub fn has_alias(&self) -> bool {
        if self.alias_owner_map.is_some() {
            return true;
        }
        if self.alias_repo_map.is_some() {
            return true;
        }
        false
    }

    pub fn get_alias_map(
        &self,
    ) -> (
        HashMap<String, String>,
        HashMap<String, HashMap<String, String>>,
    ) {
        let owner_map = match self.alias_owner_map.as_ref() {
            Some(map) => map.clone(),
            None => HashMap::new(),
        };
        let repo_map = match self.alias_repo_map.as_ref() {
            Some(map) => map.clone(),
            None => HashMap::new(),
        };
        (owner_map, repo_map)
    }

    pub fn alias_owner(&self, raw: impl AsRef<str>) -> Option<&str> {
        if let Some(alias_owner) = self.alias_owner_map.as_ref() {
            if let Some(name) = alias_owner.get(raw.as_ref()) {
                return Some(name.as_str());
            }
        }
        None
    }

    pub fn alias_repo<O, N>(&self, owner: O, name: N) -> Option<&str>
    where
        O: AsRef<str>,
        N: AsRef<str>,
    {
        if let Some(alias_repo) = self.alias_repo_map.as_ref() {
            if let Some(alias_name) = alias_repo.get(owner.as_ref()) {
                if let Some(name) = alias_name.get(name.as_ref()) {
                    return Some(name.as_str());
                }
            }
        }
        None
    }

    fn validate(&mut self) -> Result<()> {
        if let Some(token) = &self.token {
            self.token = Some(utils::expandenv(token).context("expand env for token")?);
        }

        let mut owner_alias = HashMap::new();
        let mut repo_alias = HashMap::new();
        for (owner, owner_cfg) in self.owners.iter() {
            if let Some(alias) = owner_cfg.alias.as_ref() {
                owner_alias.insert(alias.clone(), owner.clone());
            }
            if !owner_cfg.repo_alias.is_empty() {
                let map: HashMap<String, String> = owner_cfg
                    .repo_alias
                    .iter()
                    .map(|(key, val)| (val.clone(), key.clone()))
                    .collect();
                repo_alias.insert(owner.clone(), map);
            }
        }

        if !owner_alias.is_empty() {
            self.alias_owner_map = Some(owner_alias);
        }

        if !repo_alias.is_empty() {
            self.alias_repo_map = Some(repo_alias);
        }

        if self.list_limit == 0 {
            self.list_limit = defaults::list_limit();
        }
        if self.api_timeout == 0 {
            self.api_timeout = defaults::api_timeout();
        }

        Ok(())
    }
}

impl Config {
    pub fn get_raw_path() -> Result<PathBuf> {
        match env::var_os("ROXIDE_CONFIG") {
            Some(path) => Ok(PathBuf::from(path)),
            None => {
                let home = utils::get_home_dir()?;
                Ok(home.join(".config").join("roxide.toml"))
            }
        }
    }

    pub fn get_path() -> Result<Option<PathBuf>> {
        let path = Self::get_raw_path()?;

        match fs::metadata(&path) {
            Ok(meta) => {
                if meta.is_dir() {
                    bail!(
                        "config file '{}' from env is a directory, require file",
                        path.display()
                    );
                }
                Ok(Some(path))
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err).with_context(|| format!("read config file '{}'", path.display())),
        }
    }

    pub fn load() -> Result<Config> {
        let path = Self::get_path()?;
        let cfg = match path.as_ref() {
            Some(path) => Self::read(path)?,
            None => Self::default()?,
        };
        Ok(cfg)
    }

    pub fn read(path: &PathBuf) -> Result<Config> {
        let data = fs::read(path).context("read config file")?;
        let toml_str = String::from_utf8_lossy(&data);
        let mut cfg: Config = toml::from_str(&toml_str).context("parse config file toml")?;
        cfg.validate().context("validate config content")?;
        Ok(cfg)
    }

    pub fn default() -> Result<Config> {
        let mut cfg = Config {
            workspace: defaults::workspace(),
            metadir: defaults::metadir(),
            docker: defaults::docker(),
            keyword_expire: defaults::keyword_expire(),
            cmd: defaults::cmd(),
            remotes: HashMap::new(),
            release: defaults::release(),
            workflows: defaults::empty_map(),
            current_dir: None,
            now: None,
            workspace_path: None,
            meta_path: None,
        };
        cfg.validate().context("validate config content")?;
        Ok(cfg)
    }

    #[cfg(test)]
    pub fn read_data(str: &str) -> Result<Config> {
        let mut cfg: Config = toml::from_str(str).context("parse config toml")?;
        cfg.validate().context("validate config content")?;
        Ok(cfg)
    }

    pub fn validate(&mut self) -> Result<()> {
        if self.workspace.is_empty() {
            self.workspace = defaults::workspace();
        }
        self.workspace = utils::expandenv(&self.workspace).context("expand config workspace")?;
        let workspace_path = PathBuf::from(&self.workspace);
        if !workspace_path.is_absolute() {
            bail!(
                "workspace path '{}' is not an absolute path",
                self.workspace
            );
        }
        self.workspace_path = Some(workspace_path);

        if self.metadir.is_empty() {
            self.metadir = defaults::metadir();
        }
        self.metadir = utils::expandenv(&self.metadir).context("expand config metadir")?;
        let meta_path = PathBuf::from(&self.metadir);
        if !meta_path.is_absolute() {
            bail!("meta path '{}' is not an absolute path", self.metadir);
        }
        self.meta_path = Some(meta_path);

        if self.cmd.is_empty() {
            self.cmd = defaults::cmd();
        }

        for (name, remote) in self.remotes.iter_mut() {
            remote
                .validate()
                .with_context(|| format!("validate config remote '{}'", name))?;
            remote.name = Some(name.clone());
        }

        let current_dir = env::current_dir().context("get current work directory")?;
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .context("system clock set to invalid time")?;

        self.current_dir = Some(current_dir);
        self.now = Some(now.as_secs());

        Ok(())
    }

    pub fn list_remotes(&self) -> Vec<String> {
        let mut names: Vec<_> = self.remotes.keys().cloned().collect();
        names.sort();
        names
    }

    pub fn get_remote(&self, remote: impl AsRef<str>) -> Option<Cow<RemoteConfig>> {
        let remote_cfg = self.remotes.get(remote.as_ref())?;
        Some(Cow::Borrowed(remote_cfg))
    }

    pub fn must_get_remote(&self, remote: impl AsRef<str>) -> Result<Cow<RemoteConfig>> {
        match self.get_remote(remote.as_ref()) {
            Some(remote) => Ok(remote),
            None => bail!("could not find remote '{}' in config", remote.as_ref()),
        }
    }

    pub fn get_remote_or_default(&self, remote: impl AsRef<str>) -> Cow<RemoteConfig> {
        self.get_remote(remote.as_ref())
            .unwrap_or(Cow::Owned(defaults::remote(remote)))
    }

    pub fn get_owner<R, O>(&self, remote: R, owner: O) -> Option<Cow<OwnerConfig>>
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

    pub fn now(&self) -> u64 {
        self.now.unwrap()
    }

    pub fn get_workflow(&self, name: impl AsRef<str>) -> Result<Cow<'_, WorkflowConfig>> {
        Self::get_workflow_from_map(&self.workflows, name)
    }

    pub fn get_workflow_from_map(
        workflows: &HashMap<String, WorkflowConfig>,
        name: impl AsRef<str>,
    ) -> Result<Cow<'_, WorkflowConfig>> {
        let workflow = match workflows.get(name.as_ref()) {
            Some(workflow) => workflow,
            None => bail!("could not find workflow '{}'", name.as_ref()),
        };

        if workflow.include.is_empty() {
            return Ok(Cow::Borrowed(workflow));
        }

        let mut include_names = Vec::with_capacity(workflow.include.len());
        Self::insert_workflow_includes(workflows, workflow, name.as_ref(), &mut include_names)?;

        let mut workflow = workflow.clone();
        // When inserting, it needs to be reversed because we are inserting the
        // included workflow at the beginning of the original workflow. When
        // inserting at the beginning, it should be done in reverse order to ensure
        // the correct sequence.
        // For example, assuming the original workflow's steps are [5, 6, 7], and
        // the workflows to be included are [1, 2] and [3, 4], the first time
        // processing [3, 4], the insertion will result in [3, 4, 5, 6, 7]; the
        // second time processing [1, 2], the insertion will result in [1, 2, 3, 4, 5, 6, 7].
        for include_name in include_names.into_iter().rev() {
            let include_workflow = workflows.get(&include_name).unwrap();

            // Merge imported workflow to main workflow.
            let WorkflowConfig {
                env,
                steps,
                include: _,
            } = include_workflow.clone();

            workflow.env.splice(0..0, env);
            workflow.steps.splice(0..0, steps);
        }

        Ok(Cow::Owned(workflow))
    }

    fn insert_workflow_includes(
        workflows: &HashMap<String, WorkflowConfig>,
        workflow: &WorkflowConfig,
        name: impl AsRef<str>,
        includes: &mut Vec<String>,
    ) -> Result<()> {
        for include_name in workflow.include.iter() {
            // If this workflow has already been included, it should not be included
            // again to avoid the possibility of infinite circular includes.
            if includes.contains(include_name) {
                continue;
            }
            let include_workflow = match workflows.get(include_name) {
                Some(include_workflow) => include_workflow,
                None => bail!(
                    "could not find include workflow '{}' in '{}'",
                    include_name,
                    name.as_ref()
                ),
            };

            // Note that the order here must not be confused; dependencies of to
            // include must be included first before including to include itself.
            if !include_workflow.include.is_empty() {
                Self::insert_workflow_includes(
                    workflows,
                    include_workflow,
                    include_name,
                    includes,
                )?;
            }
            includes.push(include_name.clone());
        }
        Ok(())
    }

    #[cfg(test)]
    pub fn set_now(&mut self, now: u64) {
        self.now = Some(now);
    }
}

#[cfg(test)]
pub mod config_tests {
    use crate::config::*;
    use crate::{hashmap, hashmap_strings, hashset_strings};

    const TEST_CONFIG_TOML: &str = r#"
workspace = "${PWD}/_test/{NAME}/workspace"
metadir = "${PWD}/_test/{NAME}/meta"

cmd = "rox"

[remotes]

[remotes.github]
clone = "github.com"
user = "fioncat"
email = "lazycat7706@gmail.com"
ssh = false
labels = ["sync"]
provider = "github"
[remotes.github.owners.fioncat]
labels = ["pin"]
ssh = true
repo_alias = { spacenvim = "vim", roxide = "rox" }
[remotes.github.owners.kubernetes]
alias = "k8s"
labels = ["huge"]
repo_alias = { kubernetes = "k8s" }

[remotes.gitlab]
clone = "gitlab.com"
user = "test"
email = "test-email@test.com"
ssh = false
provider = "gitlab"
token = "test-token-gitlab"
cache_hours = 100
list_limit = 500
api_timeout = 30
api_domain = "gitlab.com"
[remotes.gitlab.owners.test]
labels = ["sync", "pin"]

[remotes.test]
[remotes.test.owners.golang]
on_create = ["golang"]
[remotes.test.owners.rust]
on_create = ["rust"]

[workflows]

[workflows.golang]
env = [
    {name = "MODULE_DOMAIN", from_repo = "clone"},
    {name = "MODULE_OWNER", from_repo = "owner"},
    {name = "MODULE_NAME", from_repo = "name"}
]
[[workflows.golang.steps]]
name = "main.go"
file = '''
package main

import "fmt"

func main() {
\tfmt.Println("hello, world!")
}
'''
[[workflows.golang.steps]]
name = "Init go module"
run = "go mod init ${MODULE_DOMAIN}/${MODULE_OWNER}/${MODULE_NAME}"

[workflows.rust]
[[workflows.rust.steps]]
name = "Init cargo"
run = "cargo init"

[workflows.build-go]
[[workflows.build-go.steps]]
name = "Build go"
image = "golang:latest"
run = "go build ./..."
env = [
    {name = "GO111MODULE", value = "on"}
]

"#;

    pub fn load_test_config(name: &str) -> Config {
        let toml_str = TEST_CONFIG_TOML.replace("{NAME}", name);
        let cfg = Config::read_data(toml_str.as_str()).unwrap();
        cfg
    }

    #[test]
    fn test_path() {
        let cfg = load_test_config("config_path");

        let current_dir = env::current_dir().unwrap();
        let expect_workspace = current_dir
            .join("_test")
            .join("config_path")
            .join("workspace");
        let expect_meta = current_dir.join("_test").join("config_path").join("meta");

        assert_eq!(cfg.get_workspace_dir(), &expect_workspace);
        assert_eq!(cfg.get_meta_dir(), &expect_meta);
        assert_eq!(cfg.cmd, "rox".to_string());
    }

    #[test]
    fn test_remote() {
        let cfg = load_test_config("config_remote");

        assert_eq!(cfg.remotes.len(), 3);

        let owner0 = OwnerConfig {
            alias: None,
            labels: Some(hashset_strings!["pin"]),
            on_create: None,
            repo_alias: hashmap_strings![
                "spacenvim" => "vim",
                "roxide" => "rox"
            ],
            ssh: Some(true),
        };
        let owner1 = OwnerConfig {
            alias: Some("k8s".to_string()),
            labels: Some(hashset_strings!["huge"]),
            on_create: None,
            repo_alias: hashmap_strings![
                "kubernetes" => "k8s"
            ],
            ssh: None,
        };
        let github_remote = RemoteConfig {
            clone: Some("github.com".to_string()),
            user: Some("fioncat".to_string()),
            email: Some("lazycat7706@gmail.com".to_string()),
            ssh: false,
            labels: Some(hashset_strings!["sync"]),
            provider: Some(ProviderType::Github),

            alias_owner_map: Some(hashmap_strings![
                "k8s" => "kubernetes"
            ]),
            alias_repo_map: Some(hashmap![
                "fioncat".to_string() => hashmap_strings![
                    "vim" => "spacenvim",
                    "rox" => "roxide"
                ],
                "kubernetes".to_string() => hashmap_strings![
                    "k8s" => "kubernetes"
                ]
            ]),

            api_domain: None,
            api_timeout: defaults::api_timeout(),
            cache_hours: defaults::cache_hours(),
            list_limit: defaults::list_limit(),
            token: None,

            owners: hashmap![
                "fioncat".to_string() => owner0,
                "kubernetes".to_string()  => owner1
            ],

            name: Some("github".to_string()),
        };
        assert_eq!(cfg.get_remote("github").unwrap().as_ref(), &github_remote);

        let owner2 = OwnerConfig {
            labels: Some(hashset_strings!["sync", "pin"]),

            alias: None,
            on_create: None,
            repo_alias: defaults::empty_map(),
            ssh: None,
        };
        let gitlab_remote = RemoteConfig {
            clone: Some("gitlab.com".to_string()),
            user: Some("test".to_string()),
            email: Some("test-email@test.com".to_string()),
            ssh: false,
            provider: Some(ProviderType::Gitlab),
            token: Some("test-token-gitlab".to_string()),
            cache_hours: 100,
            list_limit: 500,
            api_timeout: 30,
            api_domain: Some("gitlab.com".to_string()),
            owners: hashmap!["test".to_string() => owner2],
            labels: None,

            alias_owner_map: None,
            alias_repo_map: None,

            name: Some("gitlab".to_string()),
        };
        assert_eq!(cfg.get_remote("gitlab").unwrap().as_ref(), &gitlab_remote);

        let owner3 = OwnerConfig {
            on_create: Some(vec!["golang".to_string()]),

            alias: None,
            labels: None,
            repo_alias: defaults::empty_map(),
            ssh: None,
        };
        let owner4 = OwnerConfig {
            on_create: Some(vec!["rust".to_string()]),

            alias: None,
            labels: None,
            repo_alias: defaults::empty_map(),
            ssh: None,
        };
        let test_remote = RemoteConfig {
            clone: None,
            user: None,
            email: None,
            ssh: false,
            provider: None,
            token: None,
            api_timeout: defaults::api_timeout(),
            cache_hours: defaults::cache_hours(),
            list_limit: defaults::list_limit(),
            api_domain: None,
            owners: hashmap![
                "golang".to_string() => owner3,
                "rust".to_string() => owner4
            ],
            labels: None,

            alias_owner_map: None,
            alias_repo_map: None,

            name: Some("test".to_string()),
        };
        assert_eq!(cfg.get_remote("test").unwrap().as_ref(), &test_remote);
    }

    const TEST_MAIN_GO_CONTENT: &str = r#"package main

import "fmt"

func main() {
\tfmt.Println("hello, world!")
}
"#;

    #[test]
    fn test_workflows() {
        let cfg = load_test_config("config_workflow");

        let w0_steps = vec![
            WorkflowStep {
                name: "main.go".to_string(),
                file: Some(TEST_MAIN_GO_CONTENT.to_string()),
                run: None,
                image: None,
                env: vec![],
                ssh: None,
                work_dir: defaults::step_work_dir(),
                capture_output: None,
                docker_build: None,
                docker_push: None,
                set_env: None,
                allow_failure: false,
                os: None,
                condition: vec![],
                if_condition: None,
            },
            WorkflowStep {
                name: "Init go module".to_string(),
                file: None,
                run: Some(String::from(
                    "go mod init ${MODULE_DOMAIN}/${MODULE_OWNER}/${MODULE_NAME}",
                )),
                image: None,
                env: vec![],
                ssh: None,
                work_dir: defaults::step_work_dir(),
                capture_output: None,
                docker_build: None,
                docker_push: None,
                set_env: None,
                allow_failure: false,
                os: None,
                condition: vec![],
                if_condition: None,
            },
        ];
        let w0_env = vec![
            WorkflowEnv {
                name: "MODULE_DOMAIN".to_string(),
                from_repo: Some(WorkflowFromRepo::Clone),
                value: None,
            },
            WorkflowEnv {
                name: "MODULE_OWNER".to_string(),
                from_repo: Some(WorkflowFromRepo::Owner),
                value: None,
            },
            WorkflowEnv {
                name: "MODULE_NAME".to_string(),
                from_repo: Some(WorkflowFromRepo::Name),
                value: None,
            },
        ];
        let w0 = WorkflowConfig {
            env: w0_env,
            steps: w0_steps,
            include: vec![],
        };

        let w1_steps = vec![WorkflowStep {
            name: "Init cargo".to_string(),
            file: None,
            run: Some(String::from("cargo init")),
            image: None,
            env: vec![],
            ssh: None,
            work_dir: defaults::step_work_dir(),
            capture_output: None,
            docker_build: None,
            docker_push: None,
            set_env: None,
            allow_failure: false,
            os: None,
            condition: vec![],
            if_condition: None,
        }];
        let w1 = WorkflowConfig {
            env: vec![],
            steps: w1_steps,
            include: vec![],
        };

        let w2_steps = vec![WorkflowStep {
            name: "Build go".to_string(),
            file: None,
            image: Some("golang:latest".to_string()),
            ssh: None,
            work_dir: defaults::step_work_dir(),
            env: vec![WorkflowEnv {
                name: "GO111MODULE".to_string(),
                value: Some("on".to_string()),
                from_repo: None,
            }],
            run: Some(String::from("go build ./...")),
            capture_output: None,
            docker_build: None,
            docker_push: None,
            set_env: None,
            allow_failure: false,
            os: None,
            condition: vec![],
            if_condition: None,
        }];
        let w2 = WorkflowConfig {
            env: vec![],
            steps: w2_steps,
            include: vec![],
        };

        let wf = hashmap![
            "golang".to_string() => w0,
            "rust".to_string() => w1,
            "build-go".to_string() => w2
        ];

        assert_eq!(cfg.workflows, wf);
    }

    #[test]
    fn test_workflows_include() {
        let cases = vec![
            (
                vec!["step0", "step1", "step2"],
                vec!["job0", "job1"],
                vec![
                    ("job0", vec![], vec!["step-0-0", "step-0-1"]),
                    ("job1", vec!["job2"], vec!["step-1-0"]),
                    ("job2", vec![], vec!["step-2-0", "step-2-1"]),
                ],
                vec![
                    "step-0-0", "step-0-1", "step-2-0", "step-2-1", "step-1-0", "step0", "step1",
                    "step2",
                ],
            ),
            (
                vec![],
                vec!["job0", "job1"],
                vec![
                    ("job0", vec!["job2"], vec!["step-0-0"]),
                    ("job1", vec!["job2"], vec!["step-1-0"]),
                    ("job2", vec![], vec!["step-2-0", "step-2-1"]),
                ],
                vec!["step-2-0", "step-2-1", "step-0-0", "step-1-0"],
            ),
            (
                vec!["step0", "step1", "step2"],
                vec!["job0"],
                vec![
                    ("job0", vec!["job1"], vec!["step-0"]),
                    ("job1", vec!["job2"], vec!["step-1"]),
                    ("job2", vec!["job3"], vec!["step-2"]),
                    ("job3", vec![], vec!["step-3"]),
                ],
                vec![
                    "step-3", "step-2", "step-1", "step-0", "step0", "step1", "step2",
                ],
            ),
            (
                vec![],
                vec!["compile", "docker-build", "docker-push"],
                vec![
                    (
                        "compile",
                        vec!["get-version", "prepare-env"],
                        vec!["prepare", "check", "test"],
                    ),
                    ("docker-build", vec!["get-version"], vec!["docker-build"]),
                    ("docker-push", vec!["get-version"], vec!["docker-push"]),
                    ("get-version", vec![], vec!["get-version"]),
                    ("prepare-env", vec!["get-version"], vec!["prepare-env"]),
                ],
                vec![
                    "get-version",
                    "prepare-env",
                    "prepare",
                    "check",
                    "test",
                    "docker-build",
                    "docker-push",
                ],
            ),
        ];

        let build_workflow = |includes: Vec<&str>, steps: Vec<&str>| -> WorkflowConfig {
            WorkflowConfig {
                env: vec![],
                steps: steps
                    .into_iter()
                    .map(|name| WorkflowStep {
                        name: name.to_string(),
                        image: None,
                        ssh: None,
                        work_dir: defaults::step_work_dir(),
                        env: vec![],
                        file: None,
                        run: None,
                        capture_output: None,
                        docker_build: None,
                        docker_push: None,
                        set_env: None,
                        allow_failure: false,
                        os: None,
                        condition: vec![],
                        if_condition: None,
                    })
                    .collect(),
                include: includes.into_iter().map(|s| s.to_string()).collect(),
            }
        };

        for case in cases {
            let (main_steps, main_includes, jobs, expect) = case;
            let main_workflow = build_workflow(main_includes, main_steps);

            let mut workflows: HashMap<String, WorkflowConfig> = HashMap::new();
            workflows.insert("main".to_string(), main_workflow);

            for job in jobs {
                let (job_name, job_includes, job_steps) = job;
                let job_workflow = build_workflow(job_includes, job_steps);
                workflows.insert(job_name.to_string(), job_workflow);
            }

            let main = Config::get_workflow_from_map(&workflows, "main").unwrap();
            let steps: Vec<&str> = main.steps.iter().map(|step| step.name.as_str()).collect();

            assert_eq!(steps, expect);
        }
    }
}

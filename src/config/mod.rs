#![allow(dead_code)]

pub mod defaults;

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::path::PathBuf;
use std::time::SystemTime;
use std::{env, fs, io};

use anyhow::{bail, Context, Result};
use serde::Deserialize;

use crate::utils;

/// The basic configuration, defining some global behaviors of roxide.
#[derive(Debug, Deserialize)]
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

    /// The remotes config.
    #[serde(default = "defaults::empty_map")]
    pub remotes: HashMap<String, Remote>,

    /// The tag release rule.
    #[serde(default = "defaults::release")]
    pub release: HashMap<String, String>,

    /// Workflow can execute some pre-defined scripts on the repo.
    #[serde(default = "defaults::empty_map")]
    pub workflows: HashMap<String, Vec<WorkflowStep>>,

    #[serde(skip)]
    current_dir: Option<PathBuf>,

    #[serde(skip)]
    now: Option<u64>,
}

/// Indicates an execution step in Workflow, which can be writing a file or
/// executing a shell command.
#[derive(Debug, Deserialize)]
pub struct WorkflowStep {
    /// If the Step is to write to a file, then name is the file name. If Step
    /// is an execution command, name is the name of the step. (required)
    pub name: String,

    /// If not empty, it is the file content.
    pub file: Option<String>,

    /// If not empty, it is the command to execute.
    pub run: Option<String>,
}

/// Remote is a Git remote repository. Typical examples are Github and Gitlab.
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct Remote {
    /// The clone domain, for Github, is "github.com". If your remote Git repository
    /// is self-built, this is a private domian, such as "git.mydomain.com".
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
    pub provider: Option<Provider>,

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
    pub owners: HashMap<String, Owner>,

    #[serde(skip)]
    alias_owner_map: Option<HashMap<String, String>>,

    #[serde(skip)]
    alias_repo_map: Option<HashMap<String, HashMap<String, String>>>,
}

/// Owner configuration. Some configurations will override remote's.
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct Owner {
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
pub enum Provider {
    #[serde(rename = "github")]
    Github,
    #[serde(rename = "gitlab")]
    Gitlab,
}

impl Remote {
    pub fn has_alias(&self) -> bool {
        if let Some(_) = self.alias_owner_map {
            return true;
        }
        if let Some(_) = self.alias_repo_map {
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
        for (owner_name, owner) in self.owners.iter() {
            if let Some(alias) = owner.alias.as_ref() {
                owner_alias.insert(alias.clone(), owner_name.clone());
            }
            if !owner.repo_alias.is_empty() {
                let map: HashMap<String, String> = owner
                    .repo_alias
                    .iter()
                    .map(|(key, val)| (val.clone(), key.clone()))
                    .collect();
                repo_alias.insert(owner_name.clone(), map);
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
    pub fn get_path() -> Result<Option<PathBuf>> {
        if let Some(path) = env::var_os("ROXIDE_CONFIG") {
            let path = PathBuf::from(path);
            return match fs::metadata(&path) {
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
                Err(err) => Err(err)
                    .with_context(|| format!("read config file '{}' from env", path.display())),
            };
        }

        let home = match env::var_os("HOME") {
            Some(home) => PathBuf::from(home),
            None => bail!(
                "$HOME env not found in your system, please make sure that you are in an UNIX system"
            ),
        };
        let dir = home.join(".config").join("roxide");
        let ents = match fs::read_dir(&dir) {
            Ok(ents) => ents,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(err) => {
                return Err(err).with_context(|| format!("read config dir '{}'", dir.display()))
            }
        };
        for ent in ents {
            let ent =
                ent.with_context(|| format!("read sub entry for config dir '{}'", dir.display()))?;
            let name = ent.file_name();
            let name = match name.to_str() {
                Some(name) => name,
                None => continue,
            };
            let path = dir.join(&name);
            let meta = ent
                .metadata()
                .with_context(|| format!("read metadata for config entry '{}'", path.display()))?;
            if meta.is_dir() {
                continue;
            }

            if name == "config.yaml" || name == "config.yml" {
                return Ok(Some(path));
            }
        }

        Ok(None)
    }

    pub fn load() -> Result<Config> {
        let path = Self::get_path()?;
        let mut cfg = match path.as_ref() {
            Some(path) => {
                let file = File::open(path)
                    .with_context(|| format!("open config file '{}'", path.display()))?;
                serde_yaml::from_reader(file)
                    .with_context(|| format!("parse config file yaml '{}'", path.display()))?
            }
            None => Self::default(),
        };

        cfg.validate().context("validate config content")?;
        Ok(cfg)
    }

    pub fn default() -> Config {
        Config {
            workspace: defaults::workspace(),
            metadir: defaults::metadir(),
            cmd: defaults::cmd(),
            remotes: HashMap::new(),
            release: defaults::release(),
            workflows: defaults::empty_map(),
            current_dir: None,
            now: None,
        }
    }

    pub fn validate(&mut self) -> Result<()> {
        if self.workspace.is_empty() {
            self.workspace = defaults::workspace();
        }
        self.workspace = utils::expandenv(&self.workspace).context("expand config workspace")?;

        if self.metadir.is_empty() {
            self.metadir = defaults::metadir();
        }
        self.metadir = utils::expandenv(&self.metadir).context("expand config metadir")?;

        if self.cmd.is_empty() {
            self.cmd = defaults::cmd();
        }

        for (name, remote) in self.remotes.iter_mut() {
            remote
                .validate()
                .with_context(|| format!("validate config remote '{}'", name))?;
        }

        let current_dir = env::current_dir().context("get current work directory")?;
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .context("system clock set to invalid time")?;

        self.current_dir = Some(current_dir);
        self.now = Some(now.as_secs());

        Ok(())
    }

    pub fn get_remote<S>(&self, name: S) -> Option<Remote>
    where
        S: AsRef<str>,
    {
        match self.remotes.get(name.as_ref()) {
            Some(remote) => Some(remote.clone()),
            None => None,
        }
    }

    pub fn get_owner<R, N>(&self, remote_name: R, name: N) -> Option<Owner>
    where
        R: AsRef<str>,
        N: AsRef<str>,
    {
        match self.remotes.get(remote_name.as_ref()) {
            Some(remote) => match remote.owners.get(name.as_ref()) {
                Some(owner) => Some(owner.clone()),
                None => None,
            },
            None => None,
        }
    }

    pub fn get_current_dir(&self) -> &PathBuf {
        self.current_dir.as_ref().unwrap()
    }

    pub fn now(&self) -> u64 {
        self.now.unwrap()
    }
}

#[cfg(test)]
pub mod tests {
    use crate::{config::*, hash_set};

    pub fn get_test_config(name: &str) -> Config {
        let owner = Owner {
            alias: None,
            labels: None,
            repo_alias: HashMap::new(),
            ssh: None,
            on_create: None,
        };

        let mut github_owners = HashMap::with_capacity(2);
        github_owners.insert(String::from("fioncat"), owner.clone());
        github_owners.insert(String::from("kubernetes"), owner.clone());

        let mut gitlab_owners = HashMap::with_capacity(1);
        gitlab_owners.insert(String::from("test"), owner.clone());

        let mut remotes = HashMap::with_capacity(2);
        remotes.insert(
            String::from("github"),
            Remote {
                clone: Some(String::from("github.com")),
                user: Some(String::from("fioncat")),
                email: Some(String::from("lazycat7706@gmail.com")),
                ssh: false,
                labels: Some(hash_set!(String::from("sync"))),
                provider: Some(Provider::Github),
                token: None,
                cache_hours: 0,
                list_limit: 0,
                api_timeout: 0,
                api_domain: None,
                owners: github_owners,
                alias_owner_map: None,
                alias_repo_map: None,
            },
        );
        remotes.insert(
            String::from("gitlab"),
            Remote {
                clone: Some(String::from("gitlab.com")),
                user: Some(String::from("test-user")),
                email: Some(String::from("test-email@test.com")),
                ssh: false,
                labels: Some(hash_set!(String::from("sync"))),
                provider: Some(Provider::Gitlab),
                token: None,
                cache_hours: 0,
                list_limit: 0,
                api_timeout: 0,
                api_domain: None,
                owners: gitlab_owners,
                alias_owner_map: None,
                alias_repo_map: None,
            },
        );

        Config {
            workspace: format!("./_test/{name}/workspace"),
            metadir: format!("./_test/{name}/metadir"),
            cmd: String::from("ro"),
            remotes,
            release: HashMap::new(),
            workflows: HashMap::new(),
            current_dir: None,
            now: None,
        }
    }
}

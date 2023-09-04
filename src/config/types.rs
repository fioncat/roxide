use std::collections::{HashMap, HashSet};
use std::env;
use std::fs::{self, File};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::Deserialize;

use crate::config::default;

/// The basic configuration, defining some global behaviors of roxide.
#[derive(Debug, Deserialize)]
pub struct Base {
    /// The working directory, where all repo will be stored.
    #[serde(default = "default::workspace")]
    pub workspace: String,

    /// Store some meta data of repo, including index, cache, etc.
    #[serde(default = "default::metadir")]
    pub metadir: String,

    /// Command map.
    #[serde(default = "default::command")]
    pub command: Command,

    #[serde(default = "default::empty_map")]
    pub sync: HashMap<String, SyncRule>,

    /// Workflow can execute some pre-defined scripts on the repo.
    #[serde(default = "default::empty_map")]
    pub workflows: HashMap<String, Vec<WorkflowStep>>,

    /// The release rule
    #[serde(default = "default::release")]
    pub release: HashMap<String, String>,
}

/// The command mapping, in order to change the current directory, the `roxide`
/// command cannot be used directly, and some shell functions need to be defined.
/// This configuration defines shell function names.
#[derive(Debug, Deserialize)]
pub struct Command {
    /// The direct mapping of the `roxide` command, but on the basis of roxide,
    /// it will change the current shell directory.
    pub base: Option<String>,

    /// The mapping for the `roxide home` command, also changes the current shell
    /// directory.
    pub home: Option<String>,

    /// Some additional mappings used to jump to the repo under the specified
    /// remote, such as `zh` -> `roxide home github`.
    ///
    /// The key is remote, value is command. The command will be mapping to
    /// `roxide home {remote}`
    #[serde(default = "default::empty_map")]
    pub remotes: HashMap<String, String>,
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

#[derive(Debug, Deserialize)]
pub struct SyncRule {
    #[serde(default = "default::empty_map")]
    pub select: HashMap<String, String>,

    #[serde(default = "default::empty_set")]
    pub actions: HashSet<SyncAction>,
}

#[derive(Debug, Deserialize, Hash, PartialEq, Eq)]
pub enum SyncAction {
    #[serde(rename = "pull")]
    Pull,
    #[serde(rename = "push")]
    Push,
    #[serde(rename = "delete")]
    Delete,
    #[serde(rename = "add")]
    Add,
    #[serde(rename = "force")]
    Force,
}

/// Remote is a Git remote repository. Typical examples are Github and Gitlab.
#[derive(Debug, Deserialize)]
pub struct Remote {
    #[serde(skip)]
    pub name: String,

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
    #[serde(default = "default::disable")]
    pub ssh: bool,

    /// The remote provider, If not empty, roxide will use remote api to enhance
    /// some capabilities, such as searching repos from remote.
    ///
    /// Currently only `github` and `gitlab` are supported. If your remote is of
    /// these two types, it is strongly recommended to enable it, and it is
    /// recommended to use it with `token` to complete the authentication.
    pub provider: Option<Provider>,

    /// Uses with `provider` to authenticate when calling api.
    /// For Github, see: https://docs.github.com/en/rest/overview/authenticating-to-the-rest-api?apiVersion=2022-11-28#authenticating-with-a-personal-access-token
    /// For Gitlab, see: https://docs.gitlab.com/ee/user/profile/personal_access_tokens.html
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
    #[serde(default = "default::cache_hours")]
    pub cache_hours: u32,

    /// Some personalized configurations for different owners.
    #[serde(default = "default::empty_map")]
    pub owners: HashMap<String, Owner>,

    /// The list limit when perform searching.
    #[serde(default = "default::list_limit")]
    pub list_limit: u32,

    /// The timeout seconds when requesting remote api.
    #[serde(default = "default::api_timeout")]
    pub api_timeout: u64,

    /// API domain, only useful for Gitlab. If your Git remote is self-built, it
    /// should be set to your self-built domain host.
    pub api_domain: Option<String>,

    #[serde(skip)]
    alias_owner_map: Option<HashMap<String, String>>,

    #[serde(skip)]
    alias_repo_map: Option<HashMap<String, HashMap<String, String>>>,
}

/// Owner configuration. Some configurations will override remote's.
#[derive(Debug, Deserialize)]
pub struct Owner {
    pub alias: Option<String>,

    #[serde(default = "default::empty_map")]
    pub repo_alias: HashMap<String, String>,

    /// If not empty, override remote's ssh.
    pub ssh: Option<bool>,

    /// After cloning or creating a repo, perform some additional workflows.
    pub on_create: Option<Vec<String>>,
}

/// The remote api provider type.
#[derive(Debug, Deserialize)]
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

    pub fn alias_owner<'a>(&'a self, raw: impl AsRef<str>) -> Option<&'a str> {
        if let Some(alias_owner) = self.alias_owner_map.as_ref() {
            if let Some(name) = alias_owner.get(raw.as_ref()) {
                return Some(name.as_str());
            }
        }
        None
    }

    pub fn alias_repo<'a, S>(&'a self, owner: S, name: S) -> Option<&'a str>
    where
        S: AsRef<str>,
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
            self.token = Some(expandenv(token).context("Expand token")?);
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

        Ok(())
    }
}

impl Base {
    fn validate(&mut self) -> Result<()> {
        self.workspace = expandenv(&self.workspace).context("Expand workspace")?;
        self.metadir = expandenv(&self.metadir).context("Expand metadir")?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct Config {
    pub base: Base,

    pub dir: PathBuf,
    remotes: Vec<RemoteFile>,
}

#[derive(Debug)]
struct RemoteFile {
    remote: String,
    filename: String,
}

impl Config {
    pub fn dir() -> Result<PathBuf> {
        // The config directory, contains base and remote configs. The env's
        // priority is higher.
        match env::var_os("ROXIDE_CONFIG_DIR") {
            Some(dir) => Ok(PathBuf::from(dir)),
            // Use env "${HOME}" to get home directory, this won't be worked on
            // Windows, since we have no plan to support Windows, so it is okay.
            None => match env::var_os("HOME") {
                // Use ~/.config/roxide as the default config directory
                Some(home) => Ok(PathBuf::from(home).join(".config").join("roxide")),
                None => bail!("Could not find HOME env"),
            },
        }
    }

    pub fn read() -> Result<Config> {
        let dir = Self::dir()?;
        let mut cfg = match fs::read_dir(&dir) {
            Ok(read_dir) => {
                let mut remotes: Vec<RemoteFile> = Vec::new();
                let mut base: Option<Base> = None;
                for item in read_dir {
                    let entry = item.context("Read dir entry")?;
                    let meta = entry.metadata().context("Read config meta")?;
                    if meta.is_dir() {
                        continue;
                    }
                    // The config filename, should be "{name}.yaml|yml".
                    let filename = match entry.file_name().to_str() {
                        Some(name) => name.to_string(),
                        None => continue,
                    };
                    let name_path = Path::new(&filename);
                    // filename ext, shoud be "yaml|yml"
                    let ext = match name_path.extension() {
                        Some(ext) => match ext.to_str() {
                            Some(ext) => ext.to_string(),
                            None => continue,
                        },
                        None => continue,
                    };
                    // config name, "config" for base config, others for remotes.
                    let name = match name_path.file_stem() {
                        Some(name) => match name.to_str() {
                            Some(name) => name.to_string(),
                            None => continue,
                        },
                        None => continue,
                    };
                    match ext.as_str() {
                        "yml" | "yaml" => {}
                        // We don't care about types other than yaml, ignore.
                        _ => continue,
                    }
                    if name == "config" {
                        // If the name is "config", it means this is a base config.
                        // A restriction is also introduced here: the name of the
                        // remote cannot be "config".
                        let path = dir.join(&filename);
                        let file = File::open(&path)
                            .with_context(|| format!("Open base config file {}", path.display()))?;
                        let base_cfg =
                            serde_yaml::from_reader(file).context("Parse base config yaml")?;
                        base = Some(base_cfg);
                        continue;
                    }
                    // This is a remote config, we will not parse it now, only
                    // "lazy" parsing will be performed when it needs to be used.
                    remotes.push(RemoteFile {
                        remote: name,
                        filename,
                    });
                }
                let base = match base {
                    Some(base) => base,
                    // User didnot provide the base config, use a default one.
                    None => default::base(),
                };

                Config { base, dir, remotes }
            }
            Err(err) if err.kind() == ErrorKind::NotFound => Config {
                // The entire config directory is empty, use a completely empty
                // Config.
                base: default::base(),
                dir,
                remotes: Vec::new(),
            },
            Err(err) => {
                return Err(err).with_context(|| format!("Read config dir {}", dir.display()))
            }
        };
        cfg.base.validate().context("Validate base config")?;

        Ok(cfg)
    }

    pub fn list_remotes<'a>(&'a self) -> Vec<&'a str> {
        let mut remotes = Vec::with_capacity(self.remotes.len());
        for remote in self.remotes.iter() {
            remotes.push(remote.remote.as_str());
        }
        remotes
    }

    pub fn get_remote(&self, name: impl AsRef<str>) -> Result<Option<Remote>> {
        let remote_file = match self.remotes.iter().find(|r| r.remote.eq(name.as_ref())) {
            Some(file) => file,
            None => return Ok(None),
        };
        let path = self.dir.join(&remote_file.filename);
        let file = File::open(&path)
            .with_context(|| format!("Open remote config file {}", path.display()))?;
        let mut remote: Remote =
            serde_yaml::from_reader(&file).context("Parse remote config yaml")?;
        remote.name = name.as_ref().to_string();
        remote.validate().context("Validate remote config")?;

        Ok(Some(remote))
    }

    pub fn must_get_remote(&self, name: impl AsRef<str>) -> Result<Remote> {
        let remote = self.get_remote(name.as_ref())?;
        if let None = remote {
            bail!("Could not find remote {}", name.as_ref());
        }
        Ok(remote.unwrap())
    }
}

fn expandenv(s: &String) -> Result<String> {
    let s = shellexpand::full(s.as_str()).with_context(|| format!("Expand env for \"{s}\""))?;
    Ok(s.to_string())
}

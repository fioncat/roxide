use std::collections::HashMap;
use std::fs;
use std::io;
use std::mem::take;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::debug;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteConfig {
    #[serde(skip)]
    pub name: String,

    pub clone: Option<String>,

    pub icon: Option<String>,

    pub api: Option<RemoteAPI>,

    pub default: Option<OwnerConfig>,

    #[serde(default)]
    pub rename: HashMap<String, RenameConfig>,

    #[serde(skip)]
    pub rename_ctx: RenameContext,

    #[serde(default)]
    pub owners: HashMap<String, OwnerConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteAPI {
    pub provider: Provider,

    #[serde(default)]
    pub token: String,

    #[serde(default = "RemoteConfig::default_cache_hours")]
    pub cache_hours: u32,

    pub host: Option<String>,

    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Provider {
    #[serde(rename = "github")]
    GitHub,
    #[serde(rename = "gitlab")]
    GitLab,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct RenameConfig {
    pub name: Option<String>,

    #[serde(default)]
    pub repos: HashMap<String, String>,

    #[serde(skip)]
    pub repos_reverse: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RenameContext {
    owners: HashMap<String, RenameConfig>,
    owners_reverse: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct OwnerConfig {
    #[serde(default)]
    pub sync: bool,

    #[serde(default)]
    pub pin: bool,

    #[serde(default)]
    pub ssh: bool,

    pub user: Option<String>,
    pub email: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct OwnerConfigRef<'a> {
    pub sync: bool,

    pub pin: bool,

    pub ssh: bool,

    pub user: Option<&'a str>,
    pub email: Option<&'a str>,
}

impl RemoteConfig {
    pub fn read(dir: &Path) -> Result<Vec<Self>> {
        debug!("[config] Reading remotes config from {}", dir.display());
        let ents = match fs::read_dir(dir) {
            Ok(d) => {
                debug!("[config] Remote config dir found");
                d
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                debug!("[config] Remote config dir not found, returns empty");
                return Ok(Vec::new());
            }
            Err(e) => {
                return Err(e).with_context(|| {
                    format!("failed to read remote config dir {}", dir.display())
                });
            }
        };

        let mut remotes = Vec::new();
        for ent in ents {
            let ent = ent.context("read remote config dir entry")?;
            let file_name = ent.file_name();
            let file_name = match file_name.to_str() {
                Some(s) => s,
                None => continue,
            };
            if !file_name.ends_with(".toml") {
                continue;
            }
            let name = file_name.trim_end_matches(".toml").to_string();
            let path = PathBuf::from(dir).join(file_name);

            debug!("[config] Found remote config: {name}: {}", path.display());

            let data = fs::read_to_string(&path)
                .with_context(|| format!("failed to read remote config file {}", path.display()))?;
            let mut remote: RemoteConfig = toml::from_str(&data)
                .with_context(|| format!("failed to parse remote config {name:?}"))?;
            remote.name = name.clone();
            remote
                .validate()
                .with_context(|| format!("failed to validate remote config {name:?}"))?;
            remotes.push(remote);
        }
        remotes.sort_unstable_by(|a, b| a.name.cmp(&b.name));

        Ok(remotes)
    }

    pub fn get_owner<'a>(&'a self, owner: &str) -> OwnerConfigRef<'a> {
        let mut owner_ref = OwnerConfigRef::default();
        if let Some(ref default) = self.default {
            owner_ref.merge(default);
        }
        if let Some(owner) = self.owners.get(owner) {
            owner_ref.merge(owner);
        }
        owner_ref
    }

    pub fn get_original<'a>(&'a self, owner: &'a str, name: &'a str) -> (&'a str, &'a str) {
        self.rename_ctx.get_original(owner, name)
    }

    pub fn get_renamed<'a>(&'a self, owner: &'a str, name: &'a str) -> (&'a str, &'a str) {
        self.rename_ctx.get_renamed(owner, name)
    }

    pub(super) fn validate(&mut self) -> Result<()> {
        debug!("[config] Validating remote config: {:?}", self);
        if self.clone.is_some() {
            let clone = super::expandenv(take(&mut self.clone).unwrap());
            if clone.is_empty() {
                bail!("clone is empty");
            }
            self.clone = Some(clone);
        }

        if self.icon.is_some() {
            let icon = super::expandenv(take(&mut self.icon).unwrap());
            if icon.is_empty() {
                bail!("icon is empty");
            }
            self.icon = Some(icon);
        }

        if let Some(ref mut api) = self.api {
            api.validate().context("validate api")?;
        }

        if let Some(ref mut default) = self.default {
            default.validate().context("validate default owner")?;
        }

        for (name, owner) in &mut self.owners {
            if name.is_empty() {
                bail!("owner name is empty");
            }
            owner
                .validate()
                .with_context(|| format!("validate owner {name:?}"))?;
        }

        let rename_ctx =
            RenameContext::new(take(&mut self.rename)).context("validate rename config")?;
        self.rename_ctx = rename_ctx;

        debug!("[config] Remote validated: {:?}", self);
        Ok(())
    }

    fn default_cache_hours() -> u32 {
        24
    }
}

impl RemoteAPI {
    const MAX_CACHE_HOURS: u32 = 24 * 30 * 12; // 1 year

    fn validate(&mut self) -> Result<()> {
        // The token can be empty
        self.token = super::expandenv(take(&mut self.token));

        // The cache can be zero, means disable, but we don't allow too large value
        if self.cache_hours >= Self::MAX_CACHE_HOURS {
            bail!("cache_hours is too large");
        }

        if self.host.is_some() {
            let host = super::expandenv(take(&mut self.host).unwrap());
            if host.is_empty() {
                bail!("host is empty");
            }
            self.host = Some(host);
        }

        if self.url.is_some() {
            let url = super::expandenv(take(&mut self.url).unwrap());
            if url.is_empty() {
                bail!("url is empty");
            }
            self.url = Some(url);
        }

        Ok(())
    }
}

impl RenameConfig {
    pub fn get_original_repo<'a>(&'a self, name: &'a str) -> Option<&'a str> {
        self.repos_reverse.get(name).map(|s| s.as_str())
    }

    pub fn get_renamed_repo<'a>(&'a self, name: &'a str) -> Option<&'a str> {
        self.repos.get(name).map(|s| s.as_str())
    }
}

impl RenameContext {
    fn new(mut rename: HashMap<String, RenameConfig>) -> Result<Self> {
        let mut owners_reverse = HashMap::new();
        for (original_owner, cfg) in rename.iter_mut() {
            let renamed_owner = match cfg.name.as_ref() {
                Some(name) => name,
                None => original_owner,
            };
            if owners_reverse.contains_key(renamed_owner) {
                bail!("duplicate renamed owner name: {renamed_owner:?}");
            }
            owners_reverse.insert(renamed_owner.clone(), original_owner.clone());

            for (original_repo, renamed_repo) in cfg.repos.iter() {
                if cfg.repos_reverse.contains_key(renamed_repo) {
                    bail!(
                        "duplicate renamed repo name: {renamed_repo:?} for owner {original_owner:?}"
                    );
                }
                cfg.repos_reverse
                    .insert(renamed_repo.clone(), original_repo.clone());
            }
        }
        Ok(Self {
            owners: rename,
            owners_reverse,
        })
    }

    pub fn has(&self) -> bool {
        !self.owners.is_empty()
    }

    pub fn get_original<'a>(&'a self, owner: &'a str, name: &'a str) -> (&'a str, &'a str) {
        let (original_owner, cfg) = self.get_original_owner(owner);
        let original_name = cfg
            .map(|cfg| cfg.get_original_repo(name).unwrap_or(name))
            .unwrap_or(name);
        (original_owner, original_name)
    }

    pub fn get_original_owner<'a>(&'a self, owner: &'a str) -> (&'a str, Option<&'a RenameConfig>) {
        match self.owners_reverse.get(owner) {
            Some(original_owner) => match self.owners.get(original_owner) {
                Some(cfg) => (original_owner.as_str(), Some(cfg)),
                None => (original_owner.as_str(), None),
            },
            None => (owner, None),
        }
    }

    pub fn get_renamed<'a>(&'a self, owner: &'a str, name: &'a str) -> (&'a str, &'a str) {
        match self.owners.get(owner) {
            Some(cfg) => match cfg.repos.get(name) {
                Some(renamed_name) => (cfg.name.as_deref().unwrap_or(owner), renamed_name.as_str()),
                None => (cfg.name.as_deref().unwrap_or(owner), name),
            },
            None => (owner, name),
        }
    }
}

impl OwnerConfig {
    fn validate(&mut self) -> Result<()> {
        if self.user.is_some() {
            let user = super::expandenv(take(&mut self.user).unwrap());
            if user.is_empty() {
                bail!("user is empty");
            }
            self.user = Some(user);
        }

        if self.email.is_some() {
            let email = super::expandenv(take(&mut self.email).unwrap());
            if email.is_empty() {
                bail!("email is empty");
            }
            self.email = Some(email);
        }

        Ok(())
    }
}

impl<'a> OwnerConfigRef<'a> {
    pub fn merge(&mut self, owner: &'a OwnerConfig) {
        if owner.sync {
            self.sync = true;
        }
        if owner.pin {
            self.pin = true;
        }
        if owner.ssh {
            self.ssh = true;
        }
        if let Some(user) = owner.user.as_ref() {
            self.user = Some(user);
        }
        if let Some(email) = owner.email.as_ref() {
            self.email = Some(email);
        }
    }
}

#[cfg(test)]
pub mod tests {
    use std::env;

    use super::*;

    pub fn expect_remotes() -> Vec<RemoteConfig> {
        let github_token = env::var_os("TEST_GITHUB_TOKEN")
            .map(|s| s.to_str().unwrap().to_string())
            .unwrap_or(String::from("${TEST_GITHUB_TOKEN}"));
        let gitlab_token = env::var_os("TEST_GITLAB_TOKEN")
            .map(|s| s.to_str().unwrap().to_string())
            .unwrap_or(String::from("${TEST_GITLAB_TOKEN}"));
        vec![
            RemoteConfig {
                name: "github".to_string(),
                clone: Some("github.com".to_string()),
                icon: Some("G".to_string()),
                api: Some(RemoteAPI {
                    provider: Provider::GitHub,
                    token: github_token,
                    cache_hours: 24,
                    host: None,
                    url: None,
                }),
                default: Some(OwnerConfig {
                    user: Some("fioncat".to_string()),
                    email: Some("lazycat7706@gmail.com".to_string()),
                    ..Default::default()
                }),
                rename: HashMap::new(),
                rename_ctx: RenameContext::default(),
                owners: {
                    let mut owners = HashMap::new();
                    owners.insert(
                        "fioncat".to_string(),
                        OwnerConfig {
                            sync: true,
                            pin: true,
                            ssh: true,
                            ..Default::default()
                        },
                    );
                    owners.insert(
                        "catus".to_string(),
                        OwnerConfig {
                            sync: true,
                            pin: true,
                            ssh: true,
                            ..Default::default()
                        },
                    );
                    owners
                },
            },
            RemoteConfig {
                name: "gitlab".to_string(),
                clone: Some("git.mydomain.com".to_string()),
                icon: Some("M".to_string()),
                api: Some(RemoteAPI {
                    provider: Provider::GitLab,
                    token: gitlab_token,
                    cache_hours: 50,
                    host: Some("api.git.mydomain.com".to_string()),
                    url: Some("https://git.mydomain.com".to_string()),
                }),
                default: Some(OwnerConfig {
                    user: Some("wenqian".to_string()),
                    email: Some("wenqian@mydomain.com".to_string()),
                    sync: true,
                    pin: true,
                    ssh: true,
                }),
                rename: HashMap::new(),
                rename_ctx: RenameContext::new({
                    let mut map = HashMap::new();
                    map.insert(
                        "torpedo".to_string(),
                        RenameConfig {
                            name: Some("cube".to_string()),
                            ..Default::default()
                        },
                    );
                    map.insert(
                        "kubernetes".to_string(),
                        RenameConfig {
                            name: Some("k8s".to_string()),
                            repos: {
                                let mut repos = HashMap::new();
                                repos.insert("kubernetes".to_string(), "k8s".to_string());
                                repos.insert("kubernetes-sigs".to_string(), "k8s-sigs".to_string());
                                repos
                            },
                            ..Default::default()
                        },
                    );
                    map
                })
                .unwrap(),
                owners: HashMap::new(),
            },
            RemoteConfig {
                name: "test".to_string(),
                icon: Some("T".to_string()),
                clone: None,
                api: None,
                default: None,
                rename: HashMap::new(),
                rename_ctx: RenameContext::default(),
                owners: HashMap::new(),
            },
        ]
    }

    #[test]
    fn test_remote_config() {
        let dir = "src/config/tests/remotes";
        let remotes = RemoteConfig::read(Path::new(dir)).unwrap();
        assert_eq!(remotes, expect_remotes());
    }
}

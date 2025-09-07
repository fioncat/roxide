use std::collections::HashMap;
use std::fs;
use std::io;
use std::mem::take;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::debug;

use super::hook::HooksConfig;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteConfig {
    #[serde(skip)]
    pub name: String,

    pub clone: Option<String>,

    pub icon: Option<String>,

    pub api: Option<RemoteAPI>,

    pub default: Option<OwnerConfig>,

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
pub struct OwnerConfig {
    #[serde(default)]
    pub sync: bool,

    #[serde(default)]
    pub pin: bool,

    #[serde(default)]
    pub ssh: bool,

    pub user: Option<String>,
    pub email: Option<String>,

    #[serde(default)]
    pub on_create: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct OwnerConfigRef<'a> {
    pub sync: bool,

    pub pin: bool,

    pub ssh: bool,

    pub user: Option<&'a str>,
    pub email: Option<&'a str>,

    pub on_create: &'a [String],
}

impl RemoteConfig {
    pub fn read(dir: &Path, hooks: &HooksConfig) -> Result<Vec<Self>> {
        debug!("[config] Read remotes config from {}", dir.display());
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
                .validate(hooks)
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

    pub(super) fn validate(&mut self, hooks: &HooksConfig) -> Result<()> {
        debug!("[config] Validate remote config: {:?}", self);
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
            default.validate(hooks).context("validate default owner")?;
        }

        for (name, owner) in &mut self.owners {
            if name.is_empty() {
                bail!("owner name is empty");
            }
            owner
                .validate(hooks)
                .with_context(|| format!("validate owner {name:?}"))?;
        }

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

impl OwnerConfig {
    fn validate(&mut self, hooks: &HooksConfig) -> Result<()> {
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

        let on_create = take(&mut self.on_create);
        let mut new_on_create = Vec::with_capacity(on_create.len());
        for (idx, name) in on_create.into_iter().enumerate() {
            let name = super::expandenv(name);
            if name.is_empty() {
                bail!("on_create hook #{idx} is empty");
            }
            if !hooks.contains(&name) {
                bail!("on_create hook {name:?} not found");
            }
            new_on_create.push(name);
        }
        self.on_create = new_on_create;

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
        if !owner.on_create.is_empty() {
            self.on_create = &owner.on_create;
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
                    ..Default::default()
                }),
                owners: HashMap::new(),
            },
            RemoteConfig {
                name: "test".to_string(),
                clone: None,
                icon: Some("T".to_string()),
                api: None,
                default: None,
                owners: {
                    let mut owners = HashMap::new();
                    owners.insert(
                        "golang".to_string(),
                        OwnerConfig {
                            on_create: vec!["gomod-init".to_string()],
                            ..Default::default()
                        },
                    );
                    owners.insert(
                        "rust".to_string(),
                        OwnerConfig {
                            on_create: vec!["cargo-init".to_string()],
                            ..Default::default()
                        },
                    );
                    owners
                },
            },
        ]
    }

    #[test]
    fn test_remote_config() {
        let dir = "src/config/tests/remotes";
        let hooks_dir = "src/config/tests/hooks";
        let hooks = HooksConfig::read(Path::new(hooks_dir)).unwrap();
        let remotes = RemoteConfig::read(Path::new(dir), &hooks).unwrap();

        assert_eq!(remotes, expect_remotes());
    }
}

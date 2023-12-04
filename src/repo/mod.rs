#![allow(dead_code)]

pub mod database;

use std::{collections::HashSet, path::PathBuf, rc::Rc};

use anyhow::Result;

use crate::config::{self, Config};
use crate::utils;

#[derive(Debug, PartialEq)]
pub struct Remote {
    pub name: String,

    pub cfg: config::Remote,
}

#[derive(Debug, PartialEq)]
pub struct Owner {
    pub name: String,

    pub remote: Rc<Remote>,

    pub cfg: Option<config::Owner>,
}

#[derive(Debug, PartialEq)]
pub struct Repo {
    pub name: String,

    pub remote: Rc<Remote>,
    pub owner: Rc<Owner>,

    pub path: Option<String>,

    pub last_accessed: u64,
    pub accessed: f64,

    pub labels: Option<HashSet<Rc<String>>>,
}

pub enum NameLevel {
    Name,
    Owner,
    Remote,
    Labels,
}

impl Repo {
    pub fn new<S, O, N>(
        cfg: &Config,
        remote_name: S,
        owner_name: O,
        name: N,
        path: Option<String>,
        labels: &Option<HashSet<String>>,
    ) -> Result<Rc<Repo>>
    where
        S: AsRef<str>,
        O: AsRef<str>,
        N: AsRef<str>,
    {
        let remote_cfg = cfg.must_get_remote(remote_name.as_ref())?;
        let remote = Rc::new(Remote {
            name: remote_name.as_ref().to_string(),
            cfg: remote_cfg.clone(),
        });
        let owner = match cfg.get_owner(remote_name.as_ref(), owner_name.as_ref()) {
            Some(owner_cfg) => Rc::new(Owner {
                name: owner_name.as_ref().to_string(),
                cfg: Some(owner_cfg.clone()),
                remote: Rc::clone(&remote),
            }),
            None => Rc::new(Owner {
                name: owner_name.as_ref().to_string(),
                cfg: None,
                remote: Rc::clone(&remote),
            }),
        };

        let name = name.as_ref().to_string();

        let mut labels: Option<HashSet<String>> = match labels {
            Some(labels) => Some(labels.iter().map(|label| label.clone()).collect()),
            None => None,
        };
        if let Some(remote_labels) = remote_cfg.labels.as_ref() {
            match labels.as_mut() {
                Some(labels) => labels.extend(remote_labels.clone()),
                None => labels = Some(remote_labels.clone()),
            }
        }
        if let Some(owner_cfg) = owner.cfg.as_ref() {
            if let Some(owner_labels) = owner_cfg.labels.as_ref() {
                match labels.as_mut() {
                    Some(labels) => labels.extend(owner_labels.clone()),
                    None => labels = Some(owner_labels.clone()),
                }
            }
        }
        let labels = match labels {
            Some(labels) => Some(labels.into_iter().map(|label| Rc::new(label)).collect()),
            None => None,
        };

        Ok(Rc::new(Repo {
            name,
            remote,
            owner,
            path,
            last_accessed: cfg.now(),
            accessed: 0.0,
            labels,
        }))
    }

    pub fn get_path(&self, cfg: &Config) -> PathBuf {
        if let Some(path) = self.path.as_ref() {
            return PathBuf::from(path);
        }

        cfg.get_workspace_dir()
            .join(&self.remote.name)
            .join(&self.owner.name)
            .join(&self.name)
    }

    pub fn has_labels(&self, labels: &Option<HashSet<String>>) -> bool {
        match labels {
            Some(labels) => {
                if labels.is_empty() {
                    return true;
                }
                if let None = self.labels {
                    return false;
                }
                let repo_labels = self.labels.as_ref().unwrap();
                for label in labels {
                    if !repo_labels.contains(label) {
                        return false;
                    }
                }
                true
            }
            None => true,
        }
    }

    pub fn update(&self, cfg: &Config, labels: Option<HashSet<String>>) -> Rc<Repo> {
        let new_labels = match labels {
            Some(labels) => {
                if labels.is_empty() {
                    None
                } else {
                    Some(labels.into_iter().map(|label| Rc::new(label)).collect())
                }
            }
            None => self.labels.clone(),
        };
        Rc::new(Repo {
            remote: Rc::clone(&self.remote),
            owner: Rc::clone(&self.owner),
            name: self.name.clone(),
            path: self.path.clone(),
            last_accessed: cfg.now(),
            accessed: self.accessed + 1.0,
            labels: new_labels,
        })
    }

    pub fn score(&self, cfg: &Config) -> f64 {
        let duration = cfg.now().saturating_sub(self.last_accessed);
        if duration < utils::HOUR {
            self.accessed * 4.0
        } else if duration < utils::DAY {
            self.accessed * 2.0
        } else if duration < utils::WEEK {
            self.accessed * 0.5
        } else {
            self.accessed * 0.25
        }
    }

    pub fn clone_url(&self) -> String {
        Self::get_clone_url(self.owner.name.as_str(), self.name.as_str(), &self.remote)
    }

    pub fn get_clone_url<O, N>(owner: O, name: N, remote: &Remote) -> String
    where
        O: AsRef<str>,
        N: AsRef<str>,
    {
        if remote.cfg.has_alias() {
            let owner = match remote.cfg.alias_owner(owner.as_ref()) {
                Some(name) => name,
                None => owner.as_ref(),
            };
            let name = match remote.cfg.alias_repo(owner, name.as_ref()) {
                Some(name) => name,
                None => name.as_ref(),
            };

            return Self::get_clone_url_raw(owner, name, remote);
        }
        Self::get_clone_url_raw(owner, name, remote)
    }

    pub fn get_clone_url_raw<O, N>(owner: O, name: N, remote: &Remote) -> String
    where
        O: AsRef<str>,
        N: AsRef<str>,
    {
        let mut ssh = remote.cfg.ssh;
        if let Some(owner_cfg) = remote.cfg.owners.get(owner.as_ref()) {
            if let Some(use_ssh) = owner_cfg.ssh {
                ssh = use_ssh;
            }
        }

        let domain = match remote.cfg.clone.as_ref() {
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

    pub fn to_string(&self, level: &NameLevel) -> String {
        match level {
            NameLevel::Name => self.name.clone(),
            NameLevel::Owner => self.name_with_owner(),
            NameLevel::Remote => self.name_with_remote(),
            NameLevel::Labels => self.name_with_labels(),
        }
    }

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

    pub fn name_with_owner(&self) -> String {
        format!("{}/{}", self.owner.name, self.name)
    }

    pub fn name_with_remote(&self) -> String {
        format!("{}:{}/{}", self.remote.name, self.owner.name, self.name)
    }

    pub fn name_with_labels(&self) -> String {
        match self.labels_string() {
            Some(labels) => {
                format!(
                    "{}:{}/{}@{}",
                    self.remote.name, self.owner.name, self.name, labels
                )
            }
            None => self.name_with_remote(),
        }
    }
}

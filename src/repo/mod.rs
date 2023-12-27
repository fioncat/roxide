pub mod database;

use std::collections::HashSet;
use std::{borrow::Cow, path::PathBuf};

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

impl<'a> AsRef<Repo<'a>> for Repo<'a> {
    fn as_ref(&self) -> &Repo<'a> {
        self
    }
}

impl Repo<'_> {
    /// Retrieve the repository path.
    ///
    /// If the user explicitly specifies a path for the repository, clone it directly
    /// and return that path. If no path is specified, consider the repository to be
    /// in the workspace and generate a path using the rule:
    ///
    /// * `{workspace}/{remote}/{owner}/{name}`.
    pub fn get_path(&self, cfg: &Config) -> PathBuf {
        if let Some(path) = self.path.as_ref() {
            return PathBuf::from(path.as_ref());
        }

        cfg.get_workspace_dir()
            .join(self.remote.as_ref())
            .join(self.owner.as_ref())
            .join(self.name.as_ref())
    }

    /// Check if the repository contains the specified labels.
    pub fn contains_labels(&self, labels: &Option<HashSet<String>>) -> bool {
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
                    if !repo_labels.contains(label.as_str()) {
                        return false;
                    }
                }
                true
            }
            None => true,
        }
    }

    pub fn score(&self, cfg: &Config) -> f64 {
        database::get_score(cfg, self.last_accessed, self.accessed)
    }
}

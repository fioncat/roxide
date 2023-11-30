pub mod database;

use std::{collections::HashSet, path::PathBuf, rc::Rc};

use crate::config::{self, Config};

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

impl Repo {
    pub fn get_path(&self, cfg: &Config) -> PathBuf {
        if let Some(path) = self.path.as_ref() {
            return PathBuf::from(path);
        }

        let path = PathBuf::from(&cfg.workspace);
        path.join(&self.remote.name)
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
}

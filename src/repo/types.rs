use std::path::PathBuf;
use std::rc::Rc;

use crate::api::types::ApiUpstream;
use crate::config;
use crate::config::types::Remote;
use crate::utils;

#[derive(Debug)]
pub struct Repo {
    pub remote: Rc<String>,
    pub owner: Rc<String>,
    pub name: Rc<String>,

    pub path: Option<String>,

    pub last_accessed: u64,
    pub accessed: f64,
}

pub enum NameLevel {
    Full,
    Owner,
    Name,
}

impl PartialEq for Repo {
    fn eq(&self, other: &Self) -> bool {
        if self.remote.ne(&other.remote) {
            return false;
        }
        if self.owner.ne(&other.owner) {
            return false;
        }
        if self.name.ne(&other.name) {
            return false;
        }
        true
    }

    fn ne(&self, other: &Self) -> bool {
        !self.eq(other)
    }
}

impl Repo {
    pub fn new<S>(remote: S, owner: S, name: S, path: Option<String>) -> Rc<Repo>
    where
        S: AsRef<str>,
    {
        Rc::new(Repo {
            remote: Rc::new(remote.as_ref().to_string()),
            owner: Rc::new(owner.as_ref().to_string()),
            name: Rc::new(name.as_ref().to_string()),
            path,
            last_accessed: config::now_secs(),
            accessed: 0.0,
        })
    }

    pub fn from_upstream<S>(remote: S, upstream: ApiUpstream) -> Rc<Repo>
    where
        S: AsRef<str>,
    {
        Self::new(remote.as_ref(), &upstream.owner, &upstream.name, None)
    }

    pub fn update(&self) -> Rc<Repo> {
        Rc::new(Repo {
            remote: Rc::clone(&self.remote),
            owner: Rc::clone(&self.owner),
            name: Rc::clone(&self.name),
            path: self.path.clone(),
            last_accessed: config::now_secs(),
            accessed: self.accessed + 1.0,
        })
    }

    pub fn score(&self) -> f64 {
        let duration = config::now_secs().saturating_sub(self.last_accessed);
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

    pub fn get_path(&self) -> PathBuf {
        if let Some(path) = &self.path {
            return PathBuf::from(path);
        }

        PathBuf::from(&config::base().workspace)
            .join(self.remote.as_str())
            .join(self.owner.as_str())
            .join(self.name.as_str())
    }

    pub fn as_string(&self, level: &NameLevel) -> String {
        match level {
            NameLevel::Full => self.full_name(),
            NameLevel::Owner => self.long_name(),
            NameLevel::Name => format!("{}", self.name),
        }
    }

    pub fn long_name(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }

    pub fn full_name(&self) -> String {
        format!("{}:{}/{}", self.remote, self.owner, self.name)
    }

    pub fn clone_url(&self, remote: &Remote) -> String {
        let mut ssh = remote.ssh;
        if let Some(owner_cfg) = remote.owners.get(self.owner.as_str()) {
            if let Some(use_ssh) = owner_cfg.ssh {
                ssh = use_ssh;
            }
        }

        let domain = match &remote.clone {
            Some(domain) => domain.as_str(),
            None => "github.com",
        };

        if ssh {
            format!("git@{}:{}.git", domain, self.long_name())
        } else {
            format!("https://{}/{}.git", domain, self.long_name())
        }
    }
}

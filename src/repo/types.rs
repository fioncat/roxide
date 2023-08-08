use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::rc::Rc;

use anyhow::{bail, Context, Result};

use crate::api::types::ApiUpstream;
use crate::config::types::Remote;
use crate::utils;
use crate::{config, info};

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

    pub fn scan_workspace() -> Result<Vec<Rc<Repo>>> {
        info!("Scanning workspace");
        let mut repos = Vec::new();
        let dir = PathBuf::from(&config::base().workspace);
        let workspace = dir.clone();
        let mut stack = vec![dir];

        while !stack.is_empty() {
            let dir = stack.pop().unwrap();
            Self::scan_dir(dir, &mut repos, &workspace, &mut stack)?;
        }
        Ok(repos)
    }

    fn scan_dir(
        dir: PathBuf,
        repos: &mut Vec<Rc<Repo>>,
        workspace: &PathBuf,
        stack: &mut Vec<PathBuf>,
    ) -> Result<()> {
        let dir_read = match fs::read_dir(&dir) {
            Ok(dir_read) => dir_read,
            Err(err) if err.kind() == ErrorKind::NotFound => return Ok(()),
            Err(err) => return Err(err).with_context(|| format!("Read dir {}", dir.display())),
        };
        for entry in dir_read.into_iter() {
            let entry = entry.with_context(|| format!("Read subdir for {}", dir.display()))?;
            let subdir = dir.join(entry.file_name());
            let meta = entry
                .metadata()
                .with_context(|| format!("Read metadata for {}", subdir.display()))?;
            if !meta.is_dir() {
                continue;
            }
            let git_dir = subdir.join(".git");
            match fs::read_dir(&git_dir) {
                Ok(_) => {}
                Err(err) if err.kind() == ErrorKind::NotFound => {
                    stack.push(subdir);
                    continue;
                }
                Err(err) => {
                    return Err(err).with_context(|| format!("Read git dir {}", git_dir.display()))
                }
            }
            if !subdir.starts_with(&workspace) {
                continue;
            }
            let rel = subdir.strip_prefix(&workspace).with_context(|| {
                format!(
                    "Strip prefix for dir {}, workspace {}",
                    subdir.display(),
                    workspace.display()
                )
            })?;
            let rel = PathBuf::from(rel);
            let mut iter = rel.iter();
            let remote = match iter.next() {
                Some(s) => match s.to_str() {
                    Some(s) => s,
                    None => continue,
                },
                None => bail!(
                    "Scan found invalid rel path {}, missing remote",
                    rel.display()
                ),
            };

            let query = format!("{}", iter.collect::<PathBuf>().display());
            let query = query.trim_matches('/');
            let (owner, name) = utils::parse_query_raw(query);
            if owner.is_empty() || name.is_empty() {
                bail!(
                    "Scan found invalid rel path {}, missing owner or name",
                    rel.display()
                );
            }

            repos.push(Rc::new(Repo {
                remote: Rc::new(remote.to_string()),
                owner: Rc::new(owner),
                name: Rc::new(name),
                path: None,
                last_accessed: 0,
                accessed: 0.0,
            }));
        }
        Ok(())
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

    pub fn get_workspace_path<S>(remote: S, owner: S, name: S) -> PathBuf
    where
        S: AsRef<str>,
    {
        PathBuf::from(&config::base().workspace)
            .join(remote.as_ref())
            .join(owner.as_ref())
            .join(name.as_ref())
    }

    pub fn get_path(&self) -> PathBuf {
        if let Some(path) = &self.path {
            return PathBuf::from(path);
        }
        Self::get_workspace_path(
            self.remote.as_str(),
            self.owner.as_str(),
            self.name.as_str(),
        )
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

    pub fn get_clone_url<S>(owner: S, name: S, remote: &Remote) -> String
    where
        S: AsRef<str>,
    {
        let mut ssh = remote.ssh;
        if let Some(owner_cfg) = remote.owners.get(owner.as_ref()) {
            if let Some(use_ssh) = owner_cfg.ssh {
                ssh = use_ssh;
            }
        }

        let domain = match &remote.clone {
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

    pub fn clone_url(&self, remote: &Remote) -> String {
        Self::get_clone_url(self.owner.as_str(), self.name.as_str(), remote)
    }
}

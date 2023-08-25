use anyhow::Result;

use crate::config;
use crate::repo::database::Database;
use crate::repo::query::parse_owner;
use crate::repo::snapshot::Snapshot;
use crate::shell::{GitBranch, GitTag};

pub struct Complete {
    pub items: Vec<String>,

    pub no_space: bool,
}

impl From<Vec<String>> for Complete {
    fn from(items: Vec<String>) -> Self {
        Complete {
            items,
            no_space: false,
        }
    }
}

impl Complete {
    pub fn empty() -> Complete {
        Complete {
            items: vec![],
            no_space: false,
        }
    }

    pub fn no_space(mut self) -> Self {
        self.no_space = true;
        self
    }

    pub fn show(&self) {
        if self.no_space {
            println!("1");
        } else {
            println!("0");
        }
        for item in self.items.iter() {
            println!("{}", item);
        }
    }
}

pub fn repo(args: &[&str]) -> Result<Complete> {
    match args.len() {
        0 | 1 => {
            let remotes = config::list_remotes();
            let items: Vec<_> = remotes
                .into_iter()
                .map(|remote| remote.to_string())
                .collect();
            Ok(Complete::from(items))
        }
        2 => {
            let remote_name = &args[0];
            let db = Database::read()?;

            let query = &args[1];
            if !query.contains("/") {
                let owners = db.list_owners(remote_name);
                let items: Vec<_> = owners
                    .into_iter()
                    .map(|owner| format!("{}/", owner))
                    .collect();
                return Ok(Complete::from(items).no_space());
            }

            let (owner, _) = parse_owner(query);
            let repos = db.list_by_remote(remote_name);
            let items: Vec<_> = repos
                .into_iter()
                .filter(|repo| repo.owner.as_str().eq(&owner))
                .map(|repo| format!("{}", repo.long_name()))
                .collect();
            Ok(Complete::from(items))
        }
        _ => Ok(Complete::empty()),
    }
}

pub fn owner(args: &[&str]) -> Result<Complete> {
    match args.len() {
        0 | 1 => {
            let remotes = config::get().list_remotes();
            let items: Vec<_> = remotes
                .into_iter()
                .map(|remote| remote.to_string())
                .collect();
            Ok(Complete::from(items))
        }
        2 => {
            let remote_name = &args[0];
            let db = Database::read()?;
            let owners = db.list_owners(remote_name);
            let items: Vec<_> = owners
                .into_iter()
                .map(|owner| format!("{}/", owner))
                .collect();
            Ok(Complete::from(items).no_space())
        }
        _ => Ok(Complete::empty()),
    }
}

pub fn branch(args: &[&str]) -> Result<Complete> {
    match args.len() {
        0 | 1 => {
            let branches = GitBranch::list()?;
            let items: Vec<_> = branches
                .into_iter()
                .filter(|branch| !branch.current)
                .map(|branch| branch.name)
                .collect();
            Ok(Complete::from(items))
        }
        _ => Ok(Complete::empty()),
    }
}

pub fn tag(args: &[&str]) -> Result<Complete> {
    match args.len() {
        0 | 1 => {
            let tags = GitTag::list()?;
            let items: Vec<_> = tags.into_iter().map(|tag| tag.to_string()).collect();
            Ok(Complete::from(items))
        }
        _ => Ok(Complete::empty()),
    }
}

pub fn run(args: &[&str]) -> Result<Complete> {
    if args.is_empty() {
        return Ok(Complete::empty());
    }
    if args.len() == 1 {
        let mut names: Vec<String> = config::base()
            .workflows
            .iter()
            .map(|(key, _)| key.clone())
            .collect();
        names.sort();
        return Ok(Complete::from(names));
    }
    repo(&args[1..])
}

pub fn snapshot(args: &[&str]) -> Result<Complete> {
    match args.len() {
        0 | 1 => {
            let names = Snapshot::list()?;
            Ok(Complete::from(names))
        }
        _ => Ok(Complete::empty()),
    }
}

pub fn release(args: &[&str]) -> Result<Complete> {
    match args.len() {
        0 | 1 => {
            let mut items: Vec<_> = config::base()
                .release
                .iter()
                .map(|(key, _val)| key.clone())
                .collect();
            items.sort();
            Ok(Complete::from(items))
        }
        _ => Ok(Complete::empty()),
    }
}

pub fn remote(args: &[&str]) -> Result<Complete> {
    match args.len() {
        0 | 1 => {
            let remotes = config::list_remotes();
            let items: Vec<_> = remotes
                .into_iter()
                .map(|remote| remote.to_string())
                .collect();
            Ok(Complete::from(items))
        }
        _ => Ok(Complete::empty()),
    }
}

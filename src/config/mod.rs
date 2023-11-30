use std::collections::{HashMap, HashSet};

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub workspace: String,

    pub metadir: String,

    pub cmd: String,

    pub remotes: HashMap<String, Remote>,

    pub release: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
pub struct WorkflowStep {
    pub name: String,

    pub file: Option<String>,
    pub run: Option<String>,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct Remote {
    pub clone: Option<String>,

    pub user: Option<String>,
    pub email: Option<String>,

    pub ssh: bool,

    pub labels: Option<HashSet<String>>,

    pub provider: Option<Provider>,

    pub token: Option<String>,

    pub cache_hours: u32,

    pub list_limit: u32,

    pub api_timeout: u64,

    pub api_domain: Option<String>,

    pub owners: HashMap<String, Owner>,

    #[serde(skip)]
    alias_owner_map: Option<HashMap<String, String>>,

    #[serde(skip)]
    alias_repo_map: Option<HashMap<String, HashMap<String, String>>>,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct Owner {
    pub alias: Option<String>,

    pub labels: Option<HashSet<String>>,

    pub repo_alias: HashMap<String, String>,

    pub ssh: Option<bool>,

    pub on_create: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub enum Provider {
    #[serde(rename = "github")]
    Github,
    #[serde(rename = "gitlab")]
    Gitlab,
}

impl Config {
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
        }
    }
}

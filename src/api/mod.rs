#![allow(dead_code)]

use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub struct ApiRepo {
    pub default_branch: String,

    pub upstream: Option<ApiUpstream>,

    pub web_url: String,
}

#[derive(Debug, PartialEq, Deserialize, Serialize, Clone)]
pub struct ApiUpstream {
    pub owner: String,
    pub name: String,
    pub default_branch: String,
}

pub trait Provider {
    // list all repos for a group, the group can be owner or org in Github.
    fn list_repos(&self, owner: &str) -> Result<Vec<String>>;

    fn search_repos(&self, query: &str) -> Result<Vec<String>>;
}

#[cfg(test)]
pub mod api_tests {
    use std::collections::HashMap;

    use anyhow::bail;

    use crate::api::*;

    pub struct StaticProvider {
        repos: HashMap<String, Vec<String>>,
    }

    impl StaticProvider {
        pub fn new(repos: Vec<(&str, Vec<&str>)>) -> Box<dyn Provider> {
            let p = StaticProvider {
                repos: repos
                    .into_iter()
                    .map(|(key, value)| {
                        (
                            key.to_string(),
                            value.into_iter().map(|value| value.to_string()).collect(),
                        )
                    })
                    .collect(),
            };
            Box::new(p)
        }

        pub fn mock() -> Box<dyn Provider> {
            Self::new(vec![
                (
                    "fioncat",
                    vec!["roxide", "spacenvim", "dotfiles", "fioncat"],
                ),
                (
                    "kubernetes",
                    vec!["kubernetes", "kube-proxy", "kubelet", "kubectl"],
                ),
            ])
        }
    }

    impl Provider for StaticProvider {
        fn list_repos(&self, owner: &str) -> Result<Vec<String>> {
            match self.repos.get(owner) {
                Some(repos) => Ok(repos.clone()),
                None => bail!("could not find owner {owner}"),
            }
        }

        fn search_repos(&self, _query: &str) -> Result<Vec<String>> {
            Ok(Vec::new())
        }
    }
}

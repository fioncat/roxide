use anyhow::Result;
use console::style;
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

impl ToString for ApiUpstream {
    fn to_string(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }
}

#[derive(Debug, Clone)]
pub struct MergeOptions {
    pub owner: String,
    pub name: String,
    pub upstream: Option<ApiUpstream>,

    pub source: String,
    pub target: String,
}

impl MergeOptions {
    pub fn pretty_display(&self) -> String {
        match &self.upstream {
            Some(upstream) => format!(
                "{}:{} => {}:{}",
                style(self.to_string()).yellow(),
                style(&self.source).magenta(),
                style(upstream.to_string()).yellow(),
                style(&self.target).magenta()
            ),
            None => format!(
                "{} => {}",
                style(&self.source).magenta(),
                style(&self.target).magenta()
            ),
        }
    }
}

impl ToString for MergeOptions {
    fn to_string(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }
}

pub trait Provider {
    // list all repos for a group, the group can be owner or org in Github.
    fn list_repos(&self, owner: &str) -> Result<Vec<String>>;

    // Get default branch name.
    fn get_repo(&self, owner: &str, name: &str) -> Result<ApiRepo>;

    // Try to get URL for merge request (or PR for Github). If merge request
    // not exists, return Ok(None).
    fn get_merge(&self, merge: MergeOptions) -> Result<Option<String>>;

    // Create merge request (or PR for Github), and return its URL.
    fn create_merge(&mut self, merge: MergeOptions, title: String, body: String) -> Result<String>;

    fn search_repos(&self, query: &str) -> Result<Vec<String>>;
}

#[cfg(test)]
pub mod tests {
    use std::collections::{HashMap, HashSet};

    use anyhow::bail;

    use super::*;

    pub struct StaticProvider {
        repos: HashMap<String, Vec<String>>,

        merges: HashSet<String>,
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
                merges: HashSet::new(),
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
                None => bail!("Could not find owner {owner}"),
            }
        }

        fn get_repo(&self, owner: &str, name: &str) -> Result<ApiRepo> {
            match self.repos.get(owner) {
                Some(repos) => match repos.iter().find_map(|repo_name| {
                    if repo_name == name {
                        Some(ApiRepo {
                            default_branch: String::from("main"),
                            upstream: None,
                            web_url: String::new(),
                        })
                    } else {
                        None
                    }
                }) {
                    Some(repo) => Ok(repo),
                    None => bail!("Could not find repo {name} under {owner}"),
                },
                None => bail!("Could not find owner {owner}"),
            }
        }

        fn get_merge(&self, merge: MergeOptions) -> Result<Option<String>> {
            self.get_repo(&merge.owner, &merge.name)?;
            match self.merges.get(merge.to_string().as_str()) {
                Some(merge) => Ok(Some(merge.clone())),
                None => Ok(None),
            }
        }

        fn create_merge(&mut self, merge: MergeOptions, _: String, _: String) -> Result<String> {
            self.get_repo(&merge.owner, &merge.name)?;
            let merge = merge.to_string();
            self.merges.insert(merge.clone());
            Ok(merge)
        }

        fn search_repos(&self, _query: &str) -> Result<Vec<String>> {
            Ok(Vec::new())
        }
    }
}

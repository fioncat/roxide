use anyhow::Result;
use console::style;
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub struct ApiRepo {
    pub name: String,

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
    fn create_merge(&self, merge: MergeOptions) -> Result<String>;
}

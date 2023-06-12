use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub struct ApiRepo {
    pub name: String,

    pub default_branch: String,

    pub upstream: Option<ApiUpstream>,

    pub web_url: String,
}

#[derive(Debug, PartialEq, Deserialize, Serialize)]
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

pub struct Merge {
    pub owner: String,
    pub name: String,
    pub upstream: Option<ApiUpstream>,

    pub title: String,
    pub body: String,

    pub source: String,
    pub target: String,
}

impl ToString for Merge {
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
    fn get_merge(&self, merge: Merge) -> Result<Option<String>>;

    // Create merge request (or PR for Github), and return its URL.
    fn create_merge(&self, merge: Merge) -> Result<String>;
}

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

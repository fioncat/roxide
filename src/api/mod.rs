mod cache;
mod github;
mod gitlab;

#[cfg(test)]
mod github_test;

#[cfg(test)]
mod gitlab_test;

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{bail, Result};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};

use crate::api::cache::Cache;
use crate::api::github::Github;
use crate::api::gitlab::Gitlab;
use crate::config;
use crate::config::types::Provider as ProviderType;
use crate::config::types::Remote;

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

pub trait Provider {
    fn list_repos(&self, owner: &str) -> Result<Vec<String>>;

    fn get_repo(&self, owner: &str, name: &str) -> Result<ApiRepo>;
}

pub fn build_common_client(remote: &Remote) -> Client {
    Client::builder()
        .timeout(Duration::from_secs(remote.api_timeout))
        .build()
        .unwrap()
}

pub fn init_provider(remote: &Remote, force: bool) -> Result<Box<dyn Provider>> {
    if let None = remote.provider {
        bail!("Missing provider config for remote {}", remote.name);
    }
    let mut provider = match remote.provider.as_ref().unwrap() {
        ProviderType::Github => Github::new(remote),
        ProviderType::Gitlab => Gitlab::new(remote),
    };
    if !force && remote.cache_hours > 0 {
        let cache_dir = PathBuf::from(&config::base().metadir)
            .join("cache")
            .join(&remote.name);
        provider = Cache::new(cache_dir, remote.cache_hours as u64, provider)?;
    }
    Ok(provider)
}

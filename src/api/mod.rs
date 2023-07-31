mod cache;
mod github;
mod gitlab;
pub mod types;

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{bail, Result};
use reqwest::blocking::Client;

use crate::api::cache::Cache;
use crate::api::github::Github;
use crate::api::gitlab::Gitlab;
use crate::api::types::Provider;
use crate::config;
use crate::config::types::Provider as ProviderType;
use crate::config::types::Remote;

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
    if remote.cache_hours > 0 {
        let cache_dir = PathBuf::from(&config::base().metadir)
            .join("cache")
            .join(&remote.name);
        provider = Cache::new(cache_dir, remote.cache_hours as u64, provider, force)?;
    }
    Ok(provider)
}

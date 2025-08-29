mod cache;
mod github;

use std::borrow::Cow;
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use tokio::net::TcpStream;
use tokio::time::timeout;

use crate::db::remote_repo::RemoteRepository;

#[async_trait]
pub trait RemoteAPI: Send + Sync {
    async fn info(&self) -> Result<RemoteInfo>;

    async fn list_repos(&self, remote: &str, owner: &str) -> Result<Vec<String>>;
    async fn get_repo(
        &self,
        remote: &str,
        owner: &str,
        name: &str,
    ) -> Result<RemoteRepository<'static>>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteInfo {
    name: Cow<'static, str>,
    auth_user: Option<String>,
    ping: bool,
    cache: bool,
}

const CONNECTION_TIMEOUT: u64 = 5;

pub(super) async fn test_connection(url: &str) -> bool {
    timeout(
        Duration::from_secs(CONNECTION_TIMEOUT),
        TcpStream::connect(url),
    )
    .await
    .is_ok()
}

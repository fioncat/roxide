mod cache;
mod github;
mod gitlab;

use std::borrow::Cow;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Result, bail};
use async_trait::async_trait;
use serde::Serialize;
use tokio::net::TcpStream;
use tokio::time::timeout;

use crate::config::remote::{Provider, RemoteConfig};
use crate::db::Database;
use crate::db::remote_repo::RemoteRepository;
use crate::format;
use crate::term::list::ListItem;

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

    async fn list_pull_requests(
        &self,
        owner: &str,
        name: &str,
        head: Option<PullRequestHead>,
    ) -> Result<Vec<PullRequest>>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteInfo {
    pub name: Cow<'static, str>,
    pub auth_user: Option<String>,
    pub ping: bool,
    pub cache: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PullRequest {
    pub id: u64,

    pub base: String,
    pub head: PullRequestHead,

    pub title: String,

    pub web_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum PullRequestHead {
    #[serde(rename = "branch")]
    Branch(String),
    #[serde(rename = "repository")]
    Repository(HeadRepository),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HeadRepository {
    pub owner: String,
    pub name: String,
    pub branch: String,
}

const PULL_TITLE_LIMIT: usize = 50;

impl ListItem for PullRequest {
    fn row<'a>(&'a self, title: &str) -> Cow<'a, str> {
        match title {
            "ID" => self.id.to_string().into(),
            "Title" => {
                if self.title.len() > PULL_TITLE_LIMIT {
                    format!("{}...", &self.title[..PULL_TITLE_LIMIT]).into()
                } else {
                    Cow::Borrowed(&self.title)
                }
            }
            "Base" => Cow::Borrowed(&self.base),
            "Head" => match &self.head {
                PullRequestHead::Branch(branch) => Cow::Borrowed(branch),
                PullRequestHead::Repository(repo) => {
                    format!("{}/{}:{}", repo.owner, repo.name, repo.branch).into()
                }
            },
            "Web URL" => Cow::Borrowed(&self.web_url),
            _ => Cow::Borrowed(""),
        }
    }
}

pub fn new(
    remote: &RemoteConfig,
    db: Arc<Database>,
    force_cache: bool,
) -> Result<Arc<dyn RemoteAPI>> {
    let Some(ref api_config) = remote.api else {
        bail!("remote {:?} does not have api config", remote.name);
    };

    let mut api: Arc<dyn RemoteAPI> = match api_config.provider {
        Provider::GitHub => Arc::new(github::GitHub::new(&api_config.token)?),
        Provider::GitLab => Arc::new(gitlab::GitLab::new(&api_config.host, &api_config.token)),
    };

    if api_config.cache_hours > 0 {
        let now = format::now();
        let expire = (api_config.cache_hours as u64) * 60 * 60;
        api = Arc::new(cache::Cache {
            upstream: api,
            db,
            expire,
            force: force_cache,
            now,
        });
    }

    Ok(api)
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

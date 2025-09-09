mod cache;
mod github;
mod gitlab;

use std::borrow::Cow;
use std::fmt::Display;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Result, bail};
use async_trait::async_trait;
use console::style;
use pad::PadStr;
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

    async fn create_pull_request(
        &self,
        owner: &str,
        name: &str,
        pr: &PullRequest,
    ) -> Result<String>;
    async fn list_pull_requests(&self, opts: ListPullRequestsOptions) -> Result<Vec<PullRequest>>;

    async fn get_action(&self, owner: &str, name: &str, commit: &str) -> Result<Action>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteInfo {
    pub name: Cow<'static, str>,
    pub auth_user: Option<String>,
    pub ping: bool,
    pub cache: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ListPullRequestsOptions {
    pub owner: String,
    pub name: String,

    pub id: Option<u64>,

    pub head: Option<PullRequestHead>,
    pub base: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PullRequest {
    pub id: u64,

    pub base: String,
    pub head: PullRequestHead,

    pub title: String,

    pub web_url: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
}

impl PullRequest {
    pub fn search_items(prs: &[PullRequest]) -> Vec<String> {
        let mut items = Vec::with_capacity(prs.len());
        let mut id_len = 0;
        let mut link_len = 0;
        for pr in prs.iter() {
            let id = pr.id.to_string();
            if id.len() > id_len {
                id_len = id.len();
            }
            let link = format!("{} -> {}", pr.head, pr.base);
            if link.len() > link_len {
                link_len = link.len();
            }
            items.push((id, link));
        }

        let mut search_items = Vec::with_capacity(prs.len());
        for (idx, (id, link)) in items.into_iter().enumerate() {
            let id = id.pad_to_width_with_alignment(id_len, pad::Alignment::Left);
            let link = link.pad_to_width_with_alignment(link_len, pad::Alignment::Left);
            search_items.push(format!("[{id}] {link}  {}", prs[idx].title));
        }
        search_items
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum PullRequestHead {
    #[serde(rename = "branch")]
    Branch(String),
    #[serde(rename = "repository")]
    Repository(HeadRepository),
}

impl Display for PullRequestHead {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PullRequestHead::Branch(branch) => write!(f, "{branch}"),
            PullRequestHead::Repository(repo) => {
                write!(f, "{}/{}:{}", repo.owner, repo.name, repo.branch)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HeadRepository {
    pub owner: String,
    pub name: String,
    pub branch: String,
}

impl ListItem for PullRequest {
    fn row<'a>(&'a self, title: &str) -> Cow<'a, str> {
        match title {
            "ID" => self.id.to_string().into(),
            "Title" => Cow::Borrowed(&self.title),
            "Base" => Cow::Borrowed(&self.base),
            "Head" => format!("{}", self.head).into(),
            "Web URL" => Cow::Borrowed(&self.web_url),
            _ => Cow::Borrowed(""),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Action {
    pub web_url: String,

    pub commit_id: String,
    pub commit_message: String,

    pub user: String,
    pub email: String,

    pub job_groups: Vec<JobGroup>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct JobGroup {
    pub name: String,
    pub web_url: String,
    pub jobs: Vec<Job>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum JobStatus {
    Pending,
    Running,
    Success,
    Failed,
    Canceled,
    Skipped,
    Manual,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Job {
    pub id: u64,
    pub name: String,
    pub status: JobStatus,
    pub web_url: String,
}

impl ListItem for Job {
    fn row<'a>(&'a self, title: &str) -> Cow<'a, str> {
        match title {
            "ID" => self.id.to_string().into(),
            "Name" => Cow::Borrowed(&self.name),
            "Status" => self.status.styled_string().into(),
            _ => Cow::Borrowed(""),
        }
    }
}

impl JobStatus {
    pub fn styled_string(&self) -> String {
        match self {
            JobStatus::Pending => style("Pending").blue().to_string(),
            JobStatus::Running => style("Running").blue().to_string(),
            JobStatus::Success => style("Success").green().to_string(),
            JobStatus::Failed => style("Failed").red().to_string(),
            JobStatus::Canceled => style("Canceled").yellow().to_string(),
            JobStatus::Skipped => style("Skipped").yellow().to_string(),
            JobStatus::Manual => style("Manual").yellow().to_string(),
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

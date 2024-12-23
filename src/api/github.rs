use std::time::Duration;

use anyhow::{bail, Context, Result};
use reqwest::blocking::{Client, Request, Response};
use reqwest::{Method, Url};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::api::*;
use crate::config::RemoteConfig;

#[derive(Debug, Deserialize)]
struct Repo {
    pub name: String,
    pub full_name: String,
    pub html_url: String,

    pub source: Option<Source>,

    pub default_branch: String,
}

#[derive(Debug, Deserialize)]
struct SearchRepoResult {
    pub items: Vec<Repo>,
}

#[derive(Debug, Deserialize)]
struct Source {
    pub name: String,
    pub owner: Owner,
    pub default_branch: String,
}

#[derive(Debug, Deserialize)]
struct Owner {
    pub login: String,
}

#[derive(Debug, Deserialize)]
struct Error {
    pub message: String,
}

#[derive(Debug, Deserialize)]
struct Release {
    pub tag_name: String,
}

impl Repo {
    fn api(self) -> ApiRepo {
        let Repo {
            name: _,
            html_url,
            full_name: _,
            source,
            default_branch,
        } = self;
        let upstream = match source {
            Some(source) => Some(ApiUpstream {
                owner: source.owner.login,
                name: source.name,
                default_branch: source.default_branch,
            }),
            None => None,
        };
        ApiRepo {
            default_branch,
            upstream,
            web_url: html_url,
        }
    }
}

struct PullRequestOptions {
    owner: String,
    name: String,

    head: String,
    head_search: String,

    base: String,
}

#[derive(Debug, Serialize)]
struct PullRequestBody {
    head: String,
    base: String,

    title: String,
    body: String,
}

impl From<MergeOptions> for PullRequestOptions {
    fn from(merge: MergeOptions) -> Self {
        let MergeOptions {
            owner,
            name,
            upstream,
            source,
            target,
        } = merge;
        let head_search = format!("{owner}/{name}:{source}");

        let (head, owner, name) = match upstream {
            Some(upstream) => {
                let head = format!("{owner}:{source}");
                (head, upstream.owner, upstream.name)
            }
            None => (source, owner, name),
        };
        PullRequestOptions {
            owner,
            name,
            head,
            head_search,
            base: target,
        }
    }
}

#[derive(Debug, Deserialize)]
struct PullRequest {
    html_url: String,
}

#[derive(Debug, Deserialize)]
struct ListWorkflowRunResult {
    workflow_runs: Vec<WorkflowRun>,
}

#[derive(Debug, Deserialize)]
struct WorkflowRun {
    id: u64,
    name: String,
    html_url: String,

    head_commit: Option<WorkflowCommit>,
}

#[derive(Debug, Deserialize)]
struct WorkflowCommit {
    id: String,
    message: String,
    author: WorkflowCommitAuthor,
}

#[derive(Debug, Deserialize)]
struct WorkflowCommitAuthor {
    name: String,
    email: String,
}

#[derive(Debug, Deserialize)]
struct ListJobResult {
    jobs: Vec<Job>,
}

#[derive(Debug, Deserialize)]
struct Job {
    id: u64,
    name: String,

    status: JobStatus,
    conclusion: Option<JobConclusion>,

    html_url: String,
}

#[derive(Debug, Deserialize)]
enum JobStatus {
    #[serde(rename = "queued")]
    Queued,

    #[serde(rename = "in_progress")]
    InProgress,

    #[serde(rename = "completed")]
    Completed,

    #[serde(rename = "waiting")]
    Waiting,
}

#[derive(Debug, Deserialize)]
enum JobConclusion {
    #[serde(rename = "success")]
    Success,

    #[serde(rename = "failure")]
    Failure,

    #[serde(rename = "neutral")]
    Neutral,

    #[serde(rename = "cancelled")]
    Cancelled,

    #[serde(rename = "skipped")]
    Skipped,

    #[serde(rename = "timed_out")]
    TimedOut,

    #[serde(rename = "action_required")]
    ActionRequired,
}

impl Job {
    fn convert_status(&self) -> ActionJobStatus {
        match &self.status {
            JobStatus::Queued | JobStatus::Waiting => return ActionJobStatus::Pending,
            JobStatus::InProgress => return ActionJobStatus::Running,
            JobStatus::Completed => {}
        }

        if self.conclusion.is_none() {
            return ActionJobStatus::Failed;
        }

        match self.conclusion.as_ref().unwrap() {
            JobConclusion::Success => ActionJobStatus::Success,
            JobConclusion::Skipped => ActionJobStatus::Skipped,
            JobConclusion::ActionRequired => ActionJobStatus::WaitingForConfirm,
            JobConclusion::Cancelled => ActionJobStatus::Canceled,
            _ => ActionJobStatus::Failed,
        }
    }
}

pub struct GitHub {
    token: Option<String>,

    client: Client,

    per_page: u32,
}

impl Provider for GitHub {
    fn info(&self) -> Result<ProviderInfo> {
        let auth = self.token.is_some();
        let ping = self.execute_get_resp("").is_ok();

        Ok(ProviderInfo {
            name: format!("GitHub API {}", Self::API_VERSION),
            auth,
            ping,
        })
    }

    fn list_repos(&self, owner: &str) -> Result<Vec<String>> {
        let path = format!("users/{owner}/repos?per_page={}", self.per_page);
        let github_repos = self.execute_get::<Vec<Repo>>(&path)?;
        let repos: Vec<String> = github_repos.into_iter().map(|repo| repo.name).collect();
        Ok(repos)
    }

    fn get_repo(&self, owner: &str, name: &str) -> Result<ApiRepo> {
        let path = format!("repos/{}/{}", owner, name);
        Ok(self.execute_get::<Repo>(&path)?.api())
    }

    fn get_merge(&self, merge: MergeOptions) -> Result<Option<String>> {
        let opts: PullRequestOptions = merge.into();
        let head = urlencoding::encode(&opts.head_search);
        let base = urlencoding::encode(&opts.base);
        let path = format!(
            "repos/{}/{}/pulls?state=open&head={}&base={}",
            opts.owner, opts.name, head, base
        );
        let mut prs = self.execute_get::<Vec<PullRequest>>(&path)?;
        if prs.is_empty() {
            return Ok(None);
        }
        Ok(Some(prs.remove(0).html_url))
    }

    fn create_merge(&mut self, merge: MergeOptions, title: String, body: String) -> Result<String> {
        let opts: PullRequestOptions = merge.into();
        let path = format!("repos/{}/{}/pulls", opts.owner, opts.name);
        let body = PullRequestBody {
            head: opts.head,
            base: opts.base,
            title,
            body,
        };
        let pr = self.execute_post::<PullRequestBody, PullRequest>(&path, body)?;
        Ok(pr.html_url)
    }

    fn search_repos(&self, query: &str) -> Result<Vec<String>> {
        let path = format!("search/repositories?q={}", query);
        let result = self.execute_get::<SearchRepoResult>(&path)?;
        let repos: Vec<String> = result
            .items
            .into_iter()
            .map(|item| item.full_name)
            .collect();

        Ok(repos)
    }

    fn get_action(&self, opts: &ActionOptions) -> Result<Option<Action>> {
        let target = match &opts.target {
            ActionTarget::Commit(commit) => format!("head_sha={commit}"),
            ActionTarget::Branch(branch) => format!("branch={branch}"),
        };
        let path = format!(
            "repos/{}/{}/actions/runs?{target}&per_page=100",
            opts.owner, opts.name
        );

        let result = self.execute_get::<ListWorkflowRunResult>(&path)?;
        if result.workflow_runs.is_empty() {
            return Ok(None);
        }

        let mut commit: Option<ActionCommit> = None;

        let mut runs: Vec<ActionRun> = Vec::with_capacity(result.workflow_runs.len());

        for mut workflow_run in result.workflow_runs {
            if workflow_run.head_commit.is_none() {
                continue;
            }

            let head_commit = workflow_run.head_commit.take().unwrap();
            match commit.as_ref() {
                Some(commit) if commit.id != head_commit.id.as_str() => continue,
                None => {
                    commit = Some(ActionCommit {
                        id: head_commit.id,
                        message: head_commit.message,
                        author_name: head_commit.author.name,
                        author_email: head_commit.author.email,
                    });
                }
                _ => {}
            }

            let path = format!(
                "repos/{}/{}/actions/runs/{}/jobs",
                opts.owner, opts.name, workflow_run.id
            );
            let result = self
                .execute_get::<ListJobResult>(&path)
                .with_context(|| format!("list jobs for workflow run {}", workflow_run.id))?;
            if result.jobs.is_empty() {
                continue;
            }

            let mut jobs: Vec<ActionJob> = Vec::with_capacity(result.jobs.len());
            for workflow_job in result.jobs {
                let status = workflow_job.convert_status();
                jobs.push(ActionJob {
                    id: workflow_job.id,
                    name: workflow_job.name,
                    status,
                    url: workflow_job.html_url,
                });
            }

            runs.push(ActionRun {
                name: workflow_run.name,
                url: Some(workflow_run.html_url),
                jobs,
            });
        }

        if commit.is_none() {
            bail!("commit info from GitHub workflow runs is empty");
        }

        runs.sort_unstable_by(|a, b| a.name.cmp(&b.name));

        Ok(Some(Action {
            url: None,
            commit: commit.unwrap(),
            runs,
        }))
    }

    fn logs_job(&self, owner: &str, name: &str, id: u64, dst: &mut dyn Write) -> Result<()> {
        let path = format!("repos/{owner}/{name}/actions/jobs/{id}/logs");
        let mut resp = self.execute_get_resp(&path)?;
        resp.copy_to(dst)
            .context("read GitHub job logs response body")?;

        Ok(())
    }

    fn get_job(&self, owner: &str, name: &str, id: u64) -> Result<ActionJob> {
        let path = format!("repos/{owner}/{name}/actions/jobs/{id}");
        let job = self.execute_get::<Job>(&path)?;
        let status = job.convert_status();
        Ok(ActionJob {
            id,
            name: job.name,
            status,
            url: job.html_url,
        })
    }
}

impl GitHub {
    const API_VERSION: &'static str = "2022-11-28";

    pub fn build(remote_cfg: &RemoteConfig) -> Box<dyn Provider> {
        let client = build_common_client(remote_cfg);
        Box::new(GitHub {
            token: remote_cfg.token.clone(),
            per_page: remote_cfg.list_limit,
            client,
        })
    }

    pub fn new_empty() -> GitHub {
        let client = Client::builder()
            .timeout(Duration::from_secs_f32(20.0))
            .build()
            .unwrap();
        GitHub {
            token: None,
            per_page: 30,
            client,
        }
    }

    fn execute_get<T>(&self, path: &str) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let req = self
            .build_request(path, Method::GET, None)
            .context("build GitHub request")?;
        self.execute(req)
    }

    fn execute_post<B, R>(&self, path: &str, body: B) -> Result<R>
    where
        B: Serialize,
        R: DeserializeOwned,
    {
        let body = serde_json::to_vec(&body).context("encode GitHub request body")?;
        let req = self.build_request(path, Method::POST, Some(body))?;
        self.execute(req)
    }

    fn execute_get_resp(&self, path: &str) -> Result<Response> {
        let req = self.build_request(path, Method::GET, None)?;
        self.execute_resp(req)
    }

    fn execute<T>(&self, req: Request) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let resp = self.execute_resp(req)?;
        let data = resp.bytes().context("read GitHub response body")?;
        serde_json::from_slice(&data).context("decode GitHub response data")
    }

    fn execute_resp(&self, req: Request) -> Result<Response> {
        let resp = self.client.execute(req).context("GitHub http request")?;
        let ok = resp.status().is_success();
        if ok {
            return Ok(resp);
        }

        let data = resp.bytes().context("read GitHub response body")?;
        match serde_json::from_slice::<Error>(&data) {
            Ok(err) => bail!("GitHub api error: {}", err.message),
            Err(_err) => bail!(
                "unknown GitHub api error: {}",
                String::from_utf8(data.to_vec())
                    .context("decode GitHub response to UTF-8 string")?
            ),
        }
    }

    fn build_request(&self, path: &str, method: Method, body: Option<Vec<u8>>) -> Result<Request> {
        let url = format!("https://api.github.com/{path}");
        let url = Url::parse(url.as_str()).with_context(|| format!("parse url {url}"))?;
        let mut builder = self.client.request(method, url);
        builder = builder
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "roxide-client")
            .header("X-GitHub-Api-Version", Self::API_VERSION);
        if let Some(token) = &self.token {
            let token_value = format!("Bearer {token}");
            builder = builder.header("Authorization", token_value);
        }
        if let Some(body) = body {
            builder = builder.body(body);
        }
        builder.build().context("build request")
    }

    pub fn get_latest_tag(&self, owner: &str, name: &str) -> Result<String> {
        let path = format!("repos/{owner}/{name}/releases/latest");
        let release = self.execute_get::<Release>(&path)?;
        Ok(release.tag_name)
    }
}

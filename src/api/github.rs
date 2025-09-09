use std::borrow::Cow;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use octocrab::models::{pulls, workflows};
use octocrab::params::State;
use octocrab::{Octocrab, Page};
use serde::Serialize;
use tokio::sync::mpsc;

use crate::db::remote_repo::{RemoteRepository, RemoteUpstream};
use crate::{debug, warn};

use super::{
    Action, HeadRepository, Job, JobGroup, JobStatus, ListPullRequestsOptions, PullRequest,
    PullRequestHead, RemoteAPI, RemoteInfo,
};

pub struct GitHub {
    client: Octocrab,
    has_token: bool,
}

#[derive(Debug, Serialize)]
struct GetActionRunRequest<'a> {
    head_sha: &'a str,
    per_page: u8,
}

impl GitHub {
    const LIST_LIMIT: u8 = 100;
    const PULLS_LIMIT: u8 = 20;

    pub fn new(token: &str) -> Result<Self> {
        let mut has_token = false;
        debug!("[github] Build GitHub API client, token: {token:?}");
        let client = if token.is_empty() {
            warn!("GitHub token is empty, using unauthenticated client which may be rate limited");
            Octocrab::builder().build()?
        } else {
            has_token = true;
            Octocrab::builder().personal_token(token).build()?
        };

        Ok(Self { client, has_token })
    }

    fn convert_pull_request(owner: &str, pull: pulls::PullRequest) -> Result<PullRequest> {
        let id = pull.number;
        let Some(web_url) = pull.html_url else {
            bail!("github pull request {id} html url is empty");
        };
        let web_url = web_url.to_string();
        let base = pull.base.ref_field;
        let head_branch = pull.head.ref_field;
        let head = match pull.head.repo {
            Some(repo) => {
                let Some(head_owner) = repo.owner else {
                    bail!("github pull request {id} head repo owner is empty");
                };
                if owner != head_owner.login {
                    PullRequestHead::Repository(HeadRepository {
                        owner: head_owner.login,
                        name: repo.name,
                        branch: head_branch,
                    })
                } else {
                    PullRequestHead::Branch(head_branch)
                }
            }
            None => PullRequestHead::Branch(head_branch),
        };
        let Some(title) = pull.title else {
            bail!("github pull request {id} title is empty");
        };
        Ok(PullRequest {
            id,
            base,
            head,
            title,
            web_url,
            body: None,
        })
    }
}

#[async_trait]
impl RemoteAPI for GitHub {
    async fn info(&self) -> Result<RemoteInfo> {
        debug!("[github] Get GitHub API info");
        let mut auth_user = None;
        let ping = if self.has_token {
            let user = self
                .client
                .current()
                .user()
                .await
                .context("get current user")?;
            auth_user = Some(user.login);
            true
        } else {
            super::test_connection("api.github.com:443").await
        };
        let info = RemoteInfo {
            name: Cow::Borrowed("GitHub API"),
            auth_user,
            ping,
            cache: false,
        };
        debug!("[github] Result: {info:?}");
        Ok(info)
    }

    async fn list_repos(&self, _remote: &str, owner: &str) -> Result<Vec<String>> {
        debug!("[github] List repos for: {owner}");
        let page = self
            .client
            .users(owner)
            .repos()
            .per_page(Self::LIST_LIMIT)
            .send()
            .await
            .context("failed to fetch github user repos")?;

        let mut names = Vec::with_capacity(page.items.len());
        for item in page.items {
            names.push(item.name);
        }
        debug!("[github] Results: {names:?}");
        Ok(names)
    }

    async fn get_repo(
        &self,
        remote: &str,
        owner: &str,
        name: &str,
    ) -> Result<RemoteRepository<'static>> {
        debug!("[github] Get repo: {owner}/{name}");
        let repo = self
            .client
            .repos(owner, name)
            .get()
            .await
            .context("failed to fetch github repo")?;

        let Some(default_branch) = repo.default_branch else {
            bail!("github repo {owner}/{name} default branch is empty");
        };

        let Some(web_url) = repo.html_url.map(|u| u.to_string()) else {
            bail!("github repo {owner}/{name} html url is empty");
        };

        let upstream = if let Some(s) = repo.source {
            let Some(u_owner) = s.owner else {
                bail!("github repo {owner}/{name} upstream owner is empty");
            };
            let u_owner = u_owner.login;

            let Some(u_default_branch) = s.default_branch else {
                bail!("github repo {owner}/{name} upstream default branch is empty");
            };

            Some(RemoteUpstream {
                owner: u_owner,
                name: s.name,
                default_branch: u_default_branch,
            })
        } else {
            None
        };
        let remote_repo = RemoteRepository {
            remote: Cow::Owned(remote.to_string()),
            owner: Cow::Owned(owner.to_string()),
            name: Cow::Owned(name.to_string()),
            default_branch,
            web_url,
            upstream,
            expire_at: 0,
        };
        debug!("[github] Result: {remote_repo:?}");
        Ok(remote_repo)
    }

    async fn create_pull_request(
        &self,
        owner: &str,
        name: &str,
        pr: &PullRequest,
    ) -> Result<String> {
        debug!("[github] Create pull request: {owner}/{name}, pr: {pr:?}");

        let pulls_handler = self.client.pulls(owner, name);
        let head = match pr.head {
            PullRequestHead::Branch(ref branch) => format!("{owner}:{branch}"),
            PullRequestHead::Repository(ref repo) => format!("{}:{}", repo.owner, repo.branch),
        };
        let mut builder = pulls_handler.create(&pr.title, head, &pr.base);
        if let Some(ref body) = pr.body {
            builder = builder.body(body);
        }

        let pr = builder.send().await?;
        let Some(web_url) = pr.html_url else {
            bail!("github pull request html url is empty");
        };
        debug!("[github] Create pull request success, url: {web_url:?}");
        Ok(web_url.to_string())
    }

    async fn list_pull_requests(&self, opts: ListPullRequestsOptions) -> Result<Vec<PullRequest>> {
        debug!("[github] List pull requests: {opts:?}");

        let pulls_handler = self.client.pulls(&opts.owner, opts.name);
        if let Some(id) = opts.id {
            let pull = pulls_handler.get(id).await?;
            let result = Self::convert_pull_request(&opts.owner, pull)?;
            return Ok(vec![result]);
        }

        let mut builder = pulls_handler
            .list()
            .state(State::Open)
            .per_page(Self::PULLS_LIMIT);
        if let Some(head) = opts.head {
            let head = match head {
                PullRequestHead::Branch(branch) => format!("{}:{branch}", opts.owner),
                PullRequestHead::Repository(repo) => format!("{}:{}", repo.owner, repo.branch),
            };
            debug!("[github] Filter head: {head:?}");
            builder = builder.head(head);
        }
        if let Some(base) = opts.base {
            builder = builder.base(base);
        }

        let pulls = builder
            .send()
            .await
            .context("failed to fetch github pull requests")?;
        let mut results = vec![];
        for pull in pulls {
            results.push(Self::convert_pull_request(&opts.owner, pull)?);
        }

        debug!("[github] Results: {results:?}");
        Ok(results)
    }

    async fn get_action(&self, owner: &str, name: &str, commit: &str) -> Result<Action> {
        debug!("[github] Get action for {owner}/{name}, commit: {commit}");

        let req = GetActionRunRequest {
            head_sha: commit,
            per_page: Self::LIST_LIMIT,
        };
        let path = format!("/repos/{owner}/{name}/actions/runs");
        let runs: Page<workflows::Run> = self.client.get(path, Some(&req)).await?;

        let action = fetch_jobs(
            owner,
            name,
            self.client.clone(),
            runs.items,
            Self::LIST_LIMIT,
        )
        .await?;
        debug!("[github] Result: {action:?}");

        Ok(action)
    }

    async fn get_job_log(&self, owner: &str, name: &str, id: u64) -> Result<String> {
        let path = format!("/repos/{owner}/{name}/actions/jobs/{id}/logs");
        let data = self.client.download(&path, "").await?;
        let logs = String::from_utf8(data).context("parse job log to string")?;
        Ok(logs)
    }
}

async fn fetch_jobs(
    owner: &str,
    name: &str,
    client: Octocrab,
    workflow_runs: Vec<workflows::Run>,
    list_limit: u8,
) -> Result<Action> {
    let jobs_count = workflow_runs.len();
    let (results_tx, mut results_rx) = mpsc::channel(jobs_count);

    let tasks = workflow_runs.into_iter().enumerate().collect::<Vec<_>>();
    let tasks = Arc::new(Mutex::new(tasks));
    let action: Arc<Mutex<Option<Action>>> = Arc::new(Mutex::new(None));

    let worker = num_cpus::get();

    let owner = Arc::new(String::from(owner));
    let name = Arc::new(String::from(name));

    let mut handlers = vec![];
    for idx in 0..worker {
        let task = ConvertWorkflowTask {
            idx,
            results_tx: results_tx.clone(),
            client: client.clone(),
            owner: owner.clone(),
            name: name.clone(),
            action: action.clone(),
            tasks: tasks.clone(),
            list_limit,
        };
        let handler = tokio::spawn(async move {
            task.run().await;
        });
        handlers.push(handler);
    }

    let mut job_groups = vec![];
    loop {
        let result = results_rx.recv().await.unwrap();
        let (idx, job_group) = match result {
            Ok((idx, job_group)) => (idx, job_group),
            Err(e) => return Err(e),
        };
        job_groups.push((idx, job_group));
        if job_groups.len() == jobs_count {
            break;
        }
    }
    for handler in handlers {
        handler.await.unwrap();
    }
    debug!("[github] Fetch workflow jobs done, job groups: {job_groups:?}");
    let action = Arc::try_unwrap(action).unwrap().into_inner().unwrap();
    let Some(mut action) = action else {
        bail!("no job found for this workflow");
    };

    job_groups.sort_unstable_by(|(idx0, _), (idx1, _)| idx0.cmp(idx1));
    let job_groups = job_groups
        .into_iter()
        .map(|(_, job)| job)
        .collect::<Vec<_>>();
    action.job_groups = job_groups;
    Ok(action)
}

struct ConvertWorkflowTask {
    idx: usize,
    client: Octocrab,
    results_tx: mpsc::Sender<Result<(usize, JobGroup)>>,
    owner: Arc<String>,
    name: Arc<String>,
    action: Arc<Mutex<Option<Action>>>,
    tasks: Arc<Mutex<Vec<(usize, workflows::Run)>>>,
    list_limit: u8,
}

impl ConvertWorkflowTask {
    async fn run(&self) {
        debug!("[github] Worker {} start to fetch workflow job", self.idx);
        loop {
            let (workflow_idx, workflow_run) = {
                let mut tasks = self.tasks.lock().unwrap();
                let task = tasks.pop();
                match task {
                    Some(task) => task,
                    None => {
                        debug!("[github] Worker {} stop", self.idx);
                        return;
                    }
                }
            };
            debug!(
                "[github] Worker {}: list jobs for workflow run, id: {}, name: {}",
                self.idx, workflow_run.id, workflow_run.name
            );

            let result = self
                .client
                .workflows(self.owner.as_ref(), self.name.as_ref())
                .list_jobs(workflow_run.id)
                .per_page(self.list_limit)
                .send()
                .await;

            let run_jobs = match result {
                Ok(jobs) => jobs,
                Err(e) => {
                    self.results_tx
                        .send(Err(e).context("failed to fetch workflow job"))
                        .await
                        .unwrap();
                    continue;
                }
            };
            let mut job_group = JobGroup {
                name: workflow_run.name,
                web_url: workflow_run.html_url.to_string(),
                jobs: vec![],
            };
            for run_job in run_jobs.items {
                let job = match convert_workflow_job(run_job) {
                    Ok(job) => job,
                    Err(e) => {
                        self.results_tx
                            .send(Err(e).context("failed to convert workflow job"))
                            .await
                            .unwrap();
                        continue;
                    }
                };
                job_group.jobs.push(job);
            }
            {
                let mut action_share = self.action.lock().unwrap();
                if action_share.is_none() {
                    let action = Action {
                        web_url: workflow_run.html_url.to_string(),
                        commit_id: workflow_run.head_sha,
                        commit_message: workflow_run.head_commit.message,
                        user: workflow_run.head_commit.author.name,
                        email: workflow_run.head_commit.author.email,
                        job_groups: vec![],
                    };
                    action_share.replace(action);
                }
            }
            self.results_tx
                .send(Ok((workflow_idx, job_group)))
                .await
                .unwrap();
        }
    }
}

fn convert_workflow_job(workflow_job: workflows::Job) -> Result<Job> {
    let status = match workflow_job.status {
        workflows::Status::Pending | workflows::Status::Queued => JobStatus::Pending,
        workflows::Status::InProgress => JobStatus::Running,
        workflows::Status::Completed => {
            let Some(conclusion) = workflow_job.conclusion else {
                bail!("when job status is completed, conclusion should not be none");
            };
            match conclusion {
                workflows::Conclusion::Success => JobStatus::Success,
                workflows::Conclusion::Failure
                | workflows::Conclusion::Cancelled
                | workflows::Conclusion::TimedOut
                | workflows::Conclusion::Neutral => JobStatus::Failed,
                workflows::Conclusion::Skipped => JobStatus::Skipped,
                workflows::Conclusion::ActionRequired => JobStatus::Manual,
                _ => JobStatus::Failed,
            }
        }
        workflows::Status::Failed => JobStatus::Failed,
        _ => JobStatus::Failed,
    };
    let job = Job {
        id: workflow_job.id.0,
        name: workflow_job.name,
        status,
        web_url: workflow_job.html_url.to_string(),
    };
    debug!("[github] Convert workflow job done: {job:?}");
    Ok(job)
}

#[cfg(test)]
mod tests {
    use std::env;

    use super::*;

    fn new() -> Option<GitHub> {
        let token = env::var("TEST_GITHUB_TOKEN").ok()?;
        if token.is_empty() {
            return None;
        }
        Some(GitHub::new(&token).unwrap())
    }

    #[tokio::test]
    async fn test_info() {
        let Some(github) = new() else {
            return;
        };
        let info = github.info().await.unwrap();
        assert_eq!(
            info,
            RemoteInfo {
                name: Cow::Borrowed("GitHub API"),
                auth_user: Some("fioncat".to_string()),
                ping: true,
                cache: false,
            }
        );
    }

    #[tokio::test]
    async fn test_list_repos() {
        let Some(github) = new() else {
            return;
        };
        let repos = github.list_repos("github", "fioncat").await.unwrap();
        assert!(repos.contains(&"roxide".to_string()));
    }

    #[tokio::test]
    async fn test_get_repo() {
        let Some(github) = new() else {
            return;
        };
        let repo = github
            .get_repo("github", "fioncat", "roxide")
            .await
            .unwrap();
        assert_eq!(
            repo,
            RemoteRepository {
                remote: Cow::Owned("github".to_string()),
                owner: Cow::Owned("fioncat".to_string()),
                name: Cow::Owned("roxide".to_string()),
                default_branch: "main".to_string(),
                web_url: "https://github.com/fioncat/roxide".to_string(),
                upstream: None,
                expire_at: 0,
            }
        );

        let repo = github
            .get_repo("github", "fioncat", "kubernetes")
            .await
            .unwrap();
        assert_eq!(
            repo,
            RemoteRepository {
                remote: Cow::Owned("github".to_string()),
                owner: Cow::Owned("fioncat".to_string()),
                name: Cow::Owned("kubernetes".to_string()),
                default_branch: "master".to_string(),
                web_url: "https://github.com/fioncat/kubernetes".to_string(),
                upstream: Some(RemoteUpstream {
                    owner: "kubernetes".to_string(),
                    name: "kubernetes".to_string(),
                    default_branch: "master".to_string(),
                }),
                expire_at: 0,
            }
        );
    }
}

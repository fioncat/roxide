use std::borrow::Cow;

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use octocrab::models::{pulls, workflows};
use octocrab::params::State;
use octocrab::{Octocrab, Page};
use serde::Serialize;

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

        let mut web_url = None;
        let mut commit_id = None;
        let mut commit_message = None;
        let mut user = None;
        let mut email = None;
        let mut job_groups = vec![];
        // TODO: Use multiple threads to speed up
        for run in runs.items {
            debug!(
                "[github] List jobs for workflow run, id: {}, name: {}",
                run.id, run.name
            );

            let run_jobs = self
                .client
                .workflows(owner, name)
                .list_jobs(run.id)
                .per_page(Self::LIST_LIMIT)
                .send()
                .await?;

            if web_url.is_none() {
                web_url = Some(run.html_url.to_string());
                commit_id = Some(run.head_sha);
                commit_message = Some(run.head_commit.message);
                user = Some(run.head_commit.author.name);
                email = Some(run.head_commit.author.email);
            }

            let mut job_group = JobGroup {
                name: run.name,
                web_url: run.html_url.to_string(),
                jobs: vec![],
            };
            for run_job in run_jobs.items {
                let status = match run_job.status {
                    workflows::Status::Pending | workflows::Status::Queued => JobStatus::Pending,
                    workflows::Status::InProgress => JobStatus::Running,
                    workflows::Status::Completed => {
                        let Some(conclusion) = run_job.conclusion else {
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
                    id: run_job.id.0,
                    name: run_job.name,
                    status,
                    web_url: run_job.html_url.to_string(),
                };
                debug!("[github] Found job: {job:?}");
                job_group.jobs.push(job);
            }

            if job_group.jobs.is_empty() {
                continue;
            }
            job_groups.push(job_group);
        }

        if web_url.is_none() {
            bail!("no action run found for commit: {commit:?}");
        }

        let action = Action {
            web_url: web_url.unwrap(),
            commit_id: commit_id.unwrap(),
            commit_message: commit_message.unwrap(),
            user: user.unwrap(),
            email: email.unwrap(),
            job_groups,
        };
        debug!("[github] Result: {action:?}");
        Ok(action)
    }
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

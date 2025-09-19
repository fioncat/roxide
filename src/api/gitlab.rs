use std::borrow::Cow;
use std::collections::HashMap;

use anyhow::{Result, bail};
use async_trait::async_trait;
use gitlab::GitlabBuilder;
use gitlab::api::merge_requests::MergeRequestState;
use gitlab::api::projects::{jobs, merge_requests, pipelines};
use gitlab::api::{self, groups, projects, users};
use gitlab::api::{AsyncQuery, Pagination};
use serde::{Deserialize, Serialize};

use crate::db::remote_repo::RemoteRepository;
use crate::debug;

use super::{
    Action, Job, JobGroup, JobStatus, ListPullRequestsOptions, PullRequest, PullRequestHead,
    RemoteAPI, RemoteInfo,
};

pub struct GitLab {
    host: String,
    client_builder: GitlabBuilder,
    has_token: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct User {
    username: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Project {
    path: String,
    default_branch: String,
    web_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CreateMergeRequestResponse {
    web_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MergeRequest {
    iid: u64,
    target_branch: String,
    source_branch: String,
    title: String,
    web_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Pipeline {
    id: u64,
    web_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PipelineJob {
    id: u64,
    name: String,
    stage: String,
    status: String,
    web_url: String,
    commit: PipelineJobCommit,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PipelineJobCommit {
    id: String,
    title: String,
    author_name: String,
    author_email: String,
}

impl GitLab {
    const DEFAULT_HOST: &'static str = "https://gitlab.com";
    const DEFAULT_LIMIT: usize = 100;

    pub fn new(host: &Option<String>, token: &str) -> Self {
        debug!("[gitlab] Building GitLab API client, host: {host:?}, token: {token:?}");
        let host = host.clone().unwrap_or(String::from(Self::DEFAULT_HOST));
        let client_builder = GitlabBuilder::new(&host, token);
        let has_token = !token.is_empty();
        Self {
            host,
            client_builder,
            has_token,
        }
    }
}

#[async_trait]
impl RemoteAPI for GitLab {
    async fn info(&self) -> Result<RemoteInfo> {
        debug!("[gitlab] Getting GitLab API info");
        let client = self.client_builder.build_async().await?;

        let mut auth_user = None;

        let ping = if self.has_token {
            let endpoint = users::CurrentUser::builder().build()?;
            let user: User = endpoint.query_async(&client).await?;
            auth_user = Some(user.username);
            true
        } else {
            super::test_connection(&format!("{}:443", self.host)).await
        };

        let info = RemoteInfo {
            name: Cow::Owned(format!("GitLab: {}", self.host)),
            auth_user,
            ping,
            cache: false,
        };

        debug!("[gitlab] Result: {info:?}");
        Ok(info)
    }

    async fn list_repos(&self, _remote: &str, owner: &str) -> Result<Vec<String>> {
        debug!("[gitlab] Listing repos for: {owner}");
        let client = self.client_builder.build_async().await?;

        let endpoint = groups::projects::GroupProjects::builder()
            .group(owner)
            .build()?;
        let projects: Vec<Project> = api::paged(endpoint, Pagination::Limit(Self::DEFAULT_LIMIT))
            .query_async(&client)
            .await?;
        let repos: Vec<_> = projects.into_iter().map(|p| p.path).collect();
        debug!("[gitlab] Results: {repos:?}");
        Ok(repos)
    }

    async fn get_repo(
        &self,
        remote: &str,
        owner: &str,
        name: &str,
    ) -> Result<RemoteRepository<'static>> {
        debug!("[gitlab] Getting repo: {owner}/{name}");
        let id = format!("{owner}/{name}");
        let client = self.client_builder.build_async().await?;

        let endpoint = projects::Project::builder().project(id).build()?;
        let project: Project = endpoint.query_async(&client).await?;

        let remote_repo = RemoteRepository {
            remote: Cow::Owned(remote.to_string()),
            owner: Cow::Owned(owner.to_string()),
            name: Cow::Owned(name.to_string()),
            default_branch: project.default_branch,
            web_url: project.web_url,
            upstream: None, // TODO: Support upstream for gitlab
            expire_at: 0,
        };

        debug!("[gitlab] Result: {remote_repo:?}");
        Ok(remote_repo)
    }

    async fn create_pull_request(
        &self,
        owner: &str,
        name: &str,
        pr: &PullRequest,
    ) -> Result<String> {
        debug!("[gitlab] Creating pull request: {owner}/{name}, pr: {pr:?}");

        let client = self.client_builder.build_async().await?;
        let id = format!("{owner}/{name}");

        let mut builder = merge_requests::CreateMergeRequest::builder();
        builder.project(id).title(&pr.title).target_branch(&pr.base);
        if let Some(ref body) = pr.body {
            builder.description(body);
        }
        match pr.head {
            PullRequestHead::Branch(ref branch) => {
                builder.source_branch(branch);
            }
            PullRequestHead::Repository(_) => {
                bail!("gitlab does not support cross-repo Merge requests yet");
            }
        }

        // TODO: Allow user to set this
        builder.squash(true);
        builder.remove_source_branch(true);

        let endpoint = builder.build()?;
        let resp: CreateMergeRequestResponse = endpoint.query_async(&client).await?;
        debug!("[gitlab] Response: {resp:?}");
        Ok(resp.web_url)
    }

    async fn list_pull_requests(&self, opts: ListPullRequestsOptions) -> Result<Vec<PullRequest>> {
        debug!("[gitlab] Listing pull requests: {opts:?}");

        let client = self.client_builder.build_async().await?;
        let id = format!("{}/{}", opts.owner, opts.name);

        let mut builder = merge_requests::MergeRequests::builder();
        builder.state(MergeRequestState::Opened).project(id);
        if let Some(id) = opts.id {
            builder.iid(id);
        } else {
            if let Some(head) = opts.head {
                match head {
                    PullRequestHead::Branch(branch) => {
                        builder.source_branch(branch);
                    }
                    PullRequestHead::Repository(_) => {
                        bail!("gitlab does not support cross-repo Merge requests yet");
                    }
                }
            }
            if let Some(base) = opts.base {
                builder.target_branch(base);
            }
        }

        let endpoint = builder.build()?;

        let mrs: Vec<MergeRequest> = api::paged(endpoint, Pagination::Limit(Self::DEFAULT_LIMIT))
            .query_async(&client)
            .await?;
        debug!("[gitlab] MRs: {mrs:?}");

        let mut results = Vec::with_capacity(mrs.len());
        for mr in mrs {
            results.push(PullRequest {
                id: mr.iid,
                base: mr.target_branch,
                head: PullRequestHead::Branch(mr.source_branch),
                title: mr.title,
                web_url: mr.web_url,
                body: None,
            });
        }

        debug!("[gitlab] Results: {results:?}");
        Ok(results)
    }

    async fn get_action_optional(
        &self,
        owner: &str,
        name: &str,
        commit: &str,
    ) -> Result<Option<Action>> {
        debug!("[gitlab] Getting action: {owner}/{name}, commit: {commit}");

        let client = self.client_builder.build_async().await?;
        let id = format!("{owner}/{name}");

        let endpoint = pipelines::Pipelines::builder()
            .project(&id)
            .sha(commit)
            .build()?;
        let mut pipeline: Vec<Pipeline> = endpoint.query_async(&client).await?;
        if pipeline.is_empty() {
            return Ok(None);
        }
        let pipeline = pipeline.remove(0);
        debug!("[gitlab] Pipeline: {pipeline:?}");

        let endpoint = pipelines::PipelineJobs::builder()
            .project(id)
            .pipeline(pipeline.id)
            .build()?;

        let mut pipeline_jobs: Vec<PipelineJob> =
            api::paged(endpoint, Pagination::Limit(Self::DEFAULT_LIMIT))
                .query_async(&client)
                .await?;
        pipeline_jobs.reverse();
        debug!("[gitlab] Pipeline jobs: {pipeline_jobs:?}");

        let mut commit = None;
        let mut message = None;
        let mut user = None;
        let mut email = None;
        let mut stage_indexes: HashMap<String, usize> = HashMap::new();
        let mut job_groups: Vec<JobGroup> = Vec::new();
        for pipeline_job in pipeline_jobs {
            if commit.is_none() {
                commit = Some(pipeline_job.commit.id);
                message = Some(pipeline_job.commit.title);
                user = Some(pipeline_job.commit.author_name);
                email = Some(pipeline_job.commit.author_email);
            }

            let status = match pipeline_job.status.as_str() {
                "created" | "pending" | "waiting_for_resource" => JobStatus::Pending,
                "running" => JobStatus::Running,
                "failed" => JobStatus::Failed,
                "success" => JobStatus::Success,
                "canceled" => JobStatus::Canceled,
                "skipped" => JobStatus::Skipped,
                "manual" => JobStatus::Manual,
                _ => JobStatus::Failed,
            };

            let job = Job {
                id: pipeline_job.id,
                name: pipeline_job.name,
                status,
                web_url: pipeline_job.web_url,
            };
            debug!("[gitlab] Converting job: {job:?}");
            match stage_indexes.get(&pipeline_job.stage) {
                Some(idx) => {
                    job_groups.get_mut(*idx).unwrap().jobs.push(job);
                }
                None => {
                    let group = JobGroup {
                        name: pipeline_job.stage.clone(),
                        web_url: pipeline.web_url.clone(),
                        jobs: vec![job],
                    };
                    let idx = job_groups.len();
                    job_groups.push(group);
                    stage_indexes.insert(pipeline_job.stage, idx);
                }
            }
        }
        if commit.is_none() {
            bail!("no commit info found for action");
        }

        let action = Action {
            web_url: pipeline.web_url,
            commit_id: commit.unwrap(),
            commit_message: message.unwrap(),
            user: user.unwrap(),
            email: email.unwrap(),
            job_groups,
        };
        debug!("[gitlab] Result: {action:?}");
        Ok(Some(action))
    }

    async fn get_job_log(&self, owner: &str, name: &str, id: u64) -> Result<String> {
        debug!("[gitlab] Getting job log: {owner}/{name}, job id: {id}");
        let project = format!("{owner}/{name}");
        let client = self.client_builder.build_async().await?;
        let endpoint = jobs::JobTrace::builder().project(project).job(id).build()?;
        let logs: String = endpoint.query_async(&client).await?;
        debug!("[gitlab] Get job logs done, size: {}", logs.len());
        Ok(logs)
    }
}

#[cfg(test)]
mod tests {
    use std::env;

    use super::*;

    struct TestGitLab {
        client: GitLab,

        user: String,

        group: String,
        repo: String,

        default_branch: String,
        web_url: String,
    }

    fn new() -> Option<TestGitLab> {
        let host = env::var("TEST_GITLAB_HOST").ok()?;
        if host.is_empty() {
            return None;
        }

        let token = env::var("TEST_GITLAB_TOKEN").ok()?;
        if token.is_empty() {
            return None;
        }

        let user = env::var("TEST_GITLAB_USER").ok()?;
        if user.is_empty() {
            return None;
        }

        let group = env::var("TEST_GITLAB_GROUP").ok()?;
        if group.is_empty() {
            return None;
        }

        let repo = env::var("TEST_GITLAB_REPO").ok()?;
        if repo.is_empty() {
            return None;
        }

        let default_branch = env::var("TEST_GITLAB_REPO_DEFAULT_BRANCH").ok()?;
        if default_branch.is_empty() {
            return None;
        }

        let web_url = env::var("TEST_GITLAB_REPO_URL").ok()?;
        if web_url.is_empty() {
            return None;
        }

        Some(TestGitLab {
            client: GitLab::new(&Some(host), &token),
            user,
            group,
            repo,
            default_branch,
            web_url,
        })
    }

    #[tokio::test]
    async fn test_info() {
        let Some(gitlab) = new() else {
            return;
        };
        let info = gitlab.client.info().await.unwrap();
        assert_eq!(
            info,
            RemoteInfo {
                name: Cow::Owned(format!("GitLab: {}", gitlab.client.host)),
                auth_user: Some(gitlab.user.clone()),
                ping: true,
                cache: false,
            }
        );
    }

    #[tokio::test]
    async fn test_list_repos() {
        let Some(gitlab) = new() else {
            return;
        };
        let repos = gitlab
            .client
            .list_repos("gitlab", &gitlab.group)
            .await
            .unwrap();
        assert!(repos.contains(&gitlab.repo));
    }

    #[tokio::test]
    async fn test_get_repo() {
        let Some(gitlab) = new() else {
            return;
        };
        let repo = gitlab
            .client
            .get_repo("gitlab", &gitlab.group, &gitlab.repo)
            .await
            .unwrap();
        assert_eq!(
            repo,
            RemoteRepository {
                remote: Cow::Owned("gitlab".to_string()),
                owner: Cow::Owned(gitlab.group.clone()),
                name: Cow::Owned(gitlab.repo.clone()),
                default_branch: gitlab.default_branch.clone(),
                web_url: gitlab.web_url.clone(),
                upstream: None,
                expire_at: 0,
            }
        );
    }

    #[tokio::test]
    async fn test_pull_request() {
        // TODO: How to test PR?
    }

    #[tokio::test]
    async fn test_action() {
        // TODO: How to test action?
    }

    #[tokio::test]
    async fn test_job_log() {
        // TODO: How to test job logs?
    }
}

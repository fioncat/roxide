use std::time::Duration;

use anyhow::Result;
use clap::Args;
use console::style;
use tokio::time;

use crate::api::{Action, JobStatus, RemoteAPI};
use crate::cmd::complete::CompleteArg;
use crate::config::context::ConfigContext;
use crate::db::repo::Repository;
use crate::exec::git::commit::get_current_commit;
use crate::{cursor_up, debug, outputln, term};

#[derive(Debug, Args)]
pub struct WaitActionArgs {
    /// Wait for all jobs in the current action to complete before proceeding. This will
    /// query the Remote API every 2 seconds and display jobs that are still pending or
    /// running in real-time.
    #[arg(name = "wait", short = 'w')]
    pub enable: bool,
}

impl WaitActionArgs {
    const WAIT_INTERVAL_SECS: u64 = 2;

    pub async fn wait(
        &self,
        ctx: &ConfigContext,
        repo: &Repository,
        api: &dyn RemoteAPI,
    ) -> Result<Action> {
        let commit = get_current_commit(ctx.git().mute())?;
        debug!("[wait_action] Current commit: {commit:?}");
        if !self.enable {
            debug!("[wait_action] Wait not enabled, directly getting action");
            return self.wait_create(repo, api, &commit).await;
        }

        let mut action = self.wait_create(repo, api, &commit).await?;
        let mut reported: usize = 0;
        let mut has_title = false;
        debug!("[wait_action] Start waiting for action jobs to complete");
        loop {
            let mut runnings: Vec<(String, Vec<String>)> = Vec::new();
            for group in action.job_groups.iter() {
                let mut running_items = Vec::new();
                for job in group.jobs.iter() {
                    let item = match job.status {
                        JobStatus::Pending => style(&job.name).yellow(),
                        JobStatus::Running => style(&job.name).cyan(),
                        _ => continue,
                    };
                    running_items.push(item.to_string());
                }
                if running_items.is_empty() {
                    continue;
                }
                runnings.push((group.name.clone(), running_items));
            }
            debug!("[wait_action] Running jobs: {runnings:?}");

            for _ in 0..reported {
                cursor_up!();
            }

            if runnings.is_empty() {
                debug!("[wait_action] No running jobs, action complete");
                if has_title {
                    debug!("[wait_action] Action complete, removing title line");
                    cursor_up!();
                }
                break;
            }

            if !has_title {
                debug!("[wait_action] First time reporting running jobs, printing title");
                outputln!("Waiting for action jobs to complete...");
                has_title = true;
            }

            reported = 0;
            let width = term::width();
            for (group, jobs) in runnings {
                debug!("[wait_action] Reporting group {group} with jobs {jobs:?}");
                let line = self.render_line(group, jobs, width);
                outputln!("{line}");
                reported += 1;
            }

            time::sleep(Duration::from_secs(Self::WAIT_INTERVAL_SECS)).await;
            action = api.get_action(&repo.owner, &repo.name, &commit).await?;
        }

        debug!("[wait_action] Action jobs complete, action: {action:?}");
        Ok(action)
    }

    async fn wait_create(
        &self,
        repo: &Repository,
        api: &dyn RemoteAPI,
        commit: &str,
    ) -> Result<Action> {
        debug!("[wait_action] Waiting for action to be created");
        let mut no_created = false;
        loop {
            match api
                .get_action_optional(&repo.owner, &repo.name, commit)
                .await?
            {
                Some(action) => {
                    debug!("[wait_action] Action created: {action:?}");
                    if no_created {
                        debug!("[wait_action] Action created after waiting");
                        cursor_up!();
                    }
                    return Ok(action);
                }
                None => {
                    debug!("[wait_action] Action not created yet, retrying");
                    if !no_created {
                        debug!("[wait_action] First time no action created");
                        outputln!("Waiting for action to be created...");
                        no_created = true;
                    }
                    time::sleep(Duration::from_secs(Self::WAIT_INTERVAL_SECS)).await;
                    continue;
                }
            }
        }
    }

    fn render_line(&self, group: String, jobs: Vec<String>, width: usize) -> String {
        let group_width = console::measure_text_width(&group);
        if group_width > width {
            return ".".repeat(width);
        }
        if group_width == width {
            return group;
        }

        let head = format!("    {group}");
        let head_width = console::measure_text_width(&head);
        if head_width > width {
            return group;
        }

        let list_head = format!("   {group}: ");
        let list_head_width = console::measure_text_width(&list_head);
        if list_head_width >= width {
            return head;
        }

        let left = width - list_head_width;
        if left == 0 {
            return head;
        }

        let count = jobs.len();
        let list = term::render_list(jobs, count, left);
        format!("{list_head}{list}")
    }

    pub fn complete() -> CompleteArg {
        CompleteArg::new().short('w')
    }
}

#[cfg(test)]
mod tests {
    use crate::api::{Job, JobGroup};
    use crate::config::context;

    use super::*;

    #[tokio::test]
    async fn test_wait_action() {
        let ctx = context::tests::build_test_context("wait_action");
        let repo = Repository {
            remote: "github".to_string(),
            owner: "fioncat".to_string(),
            name: "roxide".to_string(),
            ..Default::default()
        };
        let api = ctx.get_api("github", false).unwrap();

        let args = WaitActionArgs { enable: true };

        let action = args.wait(&ctx, &repo, api.as_ref()).await.unwrap();
        assert_eq!(
            action,
            Action {
                web_url: "https://example.com/action".to_string(),
                commit_id: "test-commit".to_string(),
                commit_message: "test commit message".to_string(),
                user: "test-user".to_string(),
                email: "test-email".to_string(),
                job_groups: vec![JobGroup {
                    name: "test-job-group".to_string(),
                    web_url: "https://example.com/job-group".to_string(),
                    jobs: vec![
                        Job {
                            id: 1,
                            name: "test-job-1".to_string(),
                            status: JobStatus::Success,
                            web_url: "https://example.com/job/1".to_string(),
                        },
                        Job {
                            id: 2,
                            name: "test-job-2".to_string(),
                            status: JobStatus::Failed,
                            web_url: "https://example.com/job/2".to_string(),
                        },
                    ],
                }],
            }
        );
    }

    #[tokio::test]
    async fn test_no_wait_action() {
        let ctx = context::tests::build_test_context("no_wait_action");
        let repo = Repository {
            remote: "github".to_string(),
            owner: "fioncat".to_string(),
            name: "roxide".to_string(),
            ..Default::default()
        };
        let api = ctx.get_api("github", false).unwrap();

        let args = WaitActionArgs { enable: false };

        let action = args.wait(&ctx, &repo, api.as_ref()).await.unwrap();
        assert_eq!(
            action,
            Action {
                web_url: "https://example.com/action".to_string(),
                commit_id: "test-commit".to_string(),
                commit_message: "test commit message".to_string(),
                user: "test-user".to_string(),
                email: "test-email".to_string(),
                job_groups: vec![JobGroup {
                    name: "test-job-group".to_string(),
                    web_url: "https://example.com/job-group".to_string(),
                    jobs: vec![
                        Job {
                            id: 1,
                            name: "test-job-1".to_string(),
                            status: JobStatus::Running,
                            web_url: "https://example.com/job/1".to_string(),
                        },
                        Job {
                            id: 2,
                            name: "test-job-2".to_string(),
                            status: JobStatus::Pending,
                            web_url: "https://example.com/job/2".to_string(),
                        },
                    ],
                }],
            }
        );
    }
}

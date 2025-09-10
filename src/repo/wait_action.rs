use std::time::Duration;

use anyhow::Result;
use clap::Args;
use console::style;
use tokio::time;

use crate::api::{Action, JobStatus, RemoteAPI};
use crate::config::context::ConfigContext;
use crate::db::repo::Repository;
use crate::exec::git::commit::get_current_commit;
use crate::{cursor_up, outputln, term};

#[derive(Debug, Args)]
pub struct WaitActionArgs {
    /// Wait for all jobs in the current action to complete before proceeding. This will
    /// query the Remote API every 2 seconds and display jobs that are still pending or
    /// running in real-time.
    #[arg(long, short)]
    pub wait: bool,
}

impl WaitActionArgs {
    const WAIT_INTERVAL_SECS: u64 = 2;

    pub async fn wait(
        &self,
        ctx: &ConfigContext,
        repo: &Repository,
        api: &dyn RemoteAPI,
    ) -> Result<Action> {
        let commit = get_current_commit(ctx.git())?;
        let mut reported: usize = 0;
        if self.wait {
            outputln!("Waiting for action jobs to complete...");
        }
        loop {
            let action = api.get_action(&repo.owner, &repo.name, &commit).await?;
            if !self.wait {
                return Ok(action);
            }

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

            if runnings.is_empty() {
                for _ in 0..reported {
                    cursor_up!();
                }
                cursor_up!();
                return Ok(action);
            }

            for _ in 0..reported {
                cursor_up!();
            }
            reported = 0;
            let width = term::width();
            for (group, jobs) in runnings {
                let line = self.render_line(group, jobs, width);
                outputln!("{line}");
                reported += 1;
            }

            time::sleep(Duration::from_secs(Self::WAIT_INTERVAL_SECS)).await;
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
}

use std::collections::HashMap;
use std::io::{self, Write};
use std::thread;
use std::time::Duration;

use anyhow::{bail, Result};
use clap::Args;
use console::style;
use pad::PadStr;

use crate::api;
use crate::api::Action;
use crate::api::ActionJob;
use crate::api::ActionJobStatus;
use crate::api::ActionOptions;
use crate::api::ActionTarget;
use crate::api::Provider;
use crate::cmd::Run;
use crate::config::Config;
use crate::exec::{self, Cmd};
use crate::git::GitBranch;
use crate::repo::database::Database;
use crate::repo::Repo;
use crate::term;
use crate::utils;

/// The remote action (CI/CD) operations.
#[derive(Args)]
pub struct ActionArgs {
    /// Use the branch to get action rather than commit.
    #[clap(short, long)]
    pub branch: bool,

    /// Open the action (or job) in default browser.
    #[clap(short, long)]
    pub open: bool,

    /// Open job rather than action (only affect `-o` option).
    #[clap(short, long)]
    pub job: bool,

    /// Select failed job for opening or logging.
    #[clap(short, long)]
    pub fail: bool,

    /// Select running job for opening or logging.
    #[clap(short = 'R', long)]
    pub running: bool,

    /// Show logs of a job.
    #[clap(short, long)]
    pub logs: bool,

    /// Keep rolling logs until the job is completed (only affect `-l` option).
    /// WARNING: Because of the limitation of remote api, if your logs are huge, this
    /// will take a lot of your cpu and memory.
    #[clap(short, long)]
    pub rolling: bool,
}

impl Run for ActionArgs {
    fn run(&self, cfg: &Config) -> Result<()> {
        let db = Database::load(cfg)?;
        let repo = db.must_get_current()?;

        let provider = api::build_raw_provider(&repo.remote_cfg);
        let opts = self.get_opts(repo)?;
        drop(db);

        let action = provider.get_action(&opts)?;
        if self.open || self.logs {
            if action.is_none() {
                bail!("no action found");
            }
            let action = action.unwrap();
            if self.logs {
                return self.logs(action, provider, opts);
            }
            return self.open(action);
        }

        self.watch(action, provider, opts)
    }
}

impl ActionArgs {
    fn get_opts(&self, repo: Repo) -> Result<ActionOptions> {
        let target = if self.branch {
            let branch = GitBranch::current(true)?;
            ActionTarget::Branch(branch)
        } else {
            let sha = Cmd::git(&["rev-parse", "HEAD"]).read()?;
            ActionTarget::Commit(sha)
        };

        Ok(ActionOptions {
            owner: repo.owner.into_owned(),
            name: repo.name.into_owned(),
            target,
        })
    }

    fn watch(
        &self,
        mut action: Option<Action>,
        provider: Box<dyn Provider>,
        opts: ActionOptions,
    ) -> Result<()> {
        let retry_sleep = || {
            thread::sleep(Duration::from_millis(500));
        };

        if action.is_none() {
            eprintln!("Waiting for action to be created...");
        }
        while action.is_none() {
            let current_action = provider.get_action(&opts)?;
            if current_action.is_none() {
                retry_sleep();
                continue;
            }

            action = current_action;
            term::cursor_up();
        }
        let action = action.unwrap();
        eprintln!("{action}");

        let mut watcher = ActionWatcher::new(action, provider, opts);
        watcher.wait()
    }

    fn open(&self, action: Action) -> Result<()> {
        if self.job || self.fail {
            let job = self.select_job(action)?;
            return utils::open_url(job.url);
        }

        if let Some(url) = action.url.as_ref() {
            return utils::open_url(url);
        }

        let items: Vec<&str> = action.runs.iter().map(|run| run.name.as_str()).collect();
        let idx = exec::fzf_search(&items)?;
        let run = &action.runs[idx];

        if run.url.is_none() {
            bail!("url is missing for action run");
        }

        utils::open_url(run.url.as_ref().unwrap())
    }

    fn logs(&self, action: Action, provider: Box<dyn Provider>, opts: ActionOptions) -> Result<()> {
        let job = self.select_job(action)?;

        if !self.rolling || job.status.is_completed() {
            let mut stderr: Box<dyn Write> = Box::new(io::stderr());

            return provider.logs_job(&opts.owner, &opts.name, job.id, stderr.as_mut());
        }

        let mut full_data: Box<Vec<u8>> = Box::default();
        loop {
            let mut data: Box<Vec<u8>> = Box::new(Vec::with_capacity(512));
            provider.logs_job(&opts.owner, &opts.name, job.id, data.as_mut())?;

            if let Some(append) = data.strip_prefix(&full_data[..]) {
                eprint!("{}", String::from_utf8_lossy(append));
            }

            let updated_job = provider.get_job(&opts.owner, &opts.name, job.id)?;
            if updated_job.status.is_completed() {
                return Ok(());
            }

            full_data = data;
            thread::sleep(Duration::from_millis(500));
        }
    }

    fn select_job(&self, action: Action) -> Result<ActionJob> {
        let mut jobs: Vec<ActionJob> = Vec::with_capacity(action.runs.len());
        let mut items: Vec<String> = Vec::with_capacity(action.runs.len());
        for run in action.runs {
            for job in run.jobs {
                if self.fail && !matches!(job.status, ActionJobStatus::Failed) {
                    continue;
                }
                if self.running && !matches!(job.status, ActionJobStatus::Running) {
                    continue;
                }

                let item = format!("{}/{}", run.name, job.name);
                items.push(item);
                jobs.push(job);
            }
        }
        if jobs.is_empty() {
            if self.running {
                bail!("no running job for current action");
            }
            if self.fail {
                bail!("no failed job for current action");
            }
            bail!("no job for current action");
        }
        if jobs.len() == 1 {
            return Ok(jobs.remove(0));
        }

        let idx = exec::fzf_search(&items)?;
        let job = jobs.remove(idx);
        Ok(job)
    }
}

struct ActionWatcher {
    status_map: HashMap<u64, ActionJobStatus>,
    last_lines: usize,

    completed: bool,

    action: Action,

    provider: Box<dyn Provider>,

    opts: ActionOptions,
}

impl ActionWatcher {
    fn new(action: Action, provider: Box<dyn Provider>, opts: ActionOptions) -> Self {
        ActionWatcher {
            status_map: HashMap::new(),
            last_lines: 0,
            completed: false,
            action,
            provider,
            opts,
        }
    }

    fn wait(&mut self) -> Result<()> {
        while !self.completed {
            let updated = self.update_status();
            if updated {
                self.display();
            }

            if !self.completed {
                self.next()?;
            }
        }

        Ok(())
    }

    fn update_status(&mut self) -> bool {
        let mut completed_count = 0;
        let mut jobs_count = 0;
        let mut updated = false;

        for run in self.action.runs.iter() {
            for job in run.jobs.iter() {
                if job.status.is_completed() {
                    completed_count += 1;
                }
                jobs_count += 1;
                let update_status = match self.status_map.get(&job.id) {
                    Some(status) if &job.status != status => true,
                    None => true,
                    _ => false,
                };
                if update_status {
                    updated = true;
                    self.status_map.insert(job.id, job.status);
                }
            }
        }

        self.completed = completed_count == jobs_count;
        updated
    }

    fn display(&mut self) {
        for _ in 0..self.last_lines {
            term::cursor_up();
        }

        self.last_lines = 0;
        for run in self.action.runs.iter() {
            eprintln!();
            eprintln!("{}", style(&run.name).bold().underlined());
            self.last_lines += 2;

            let mut pad = 0;
            for job in run.jobs.iter() {
                if job.name.len() > pad {
                    pad = job.name.len();
                }
            }
            pad += 1;

            for job in run.jobs.iter() {
                let name = job
                    .name
                    .pad_to_width_with_alignment(pad, pad::Alignment::Left);
                eprintln!("{name} {}", job.status);
                self.last_lines += 1;
            }
        }
    }

    fn next(&mut self) -> Result<()> {
        Self::retry_sleep();
        let current_action = self.provider.get_action(&self.opts)?;
        if current_action.is_none() {
            bail!("action was removed during watching");
        }
        self.action = current_action.unwrap();
        Ok(())
    }

    fn retry_sleep() {
        thread::sleep(Duration::from_millis(100));
    }
}

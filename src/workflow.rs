use std::borrow::Cow;
use std::path::Path;
use std::sync::Arc;
use std::{collections::HashMap, path::PathBuf};

use anyhow::Result;
use anyhow::{bail, Context};
use console::style;

use crate::batch::Task;
use crate::config::Config;
use crate::config::Docker;
use crate::config::WorkflowCondition;
use crate::config::WorkflowConfig;
use crate::config::WorkflowEnv;
use crate::config::WorkflowFromRepo;
use crate::config::WorkflowOS;
use crate::config::WorkflowStep;
use crate::info;
use crate::repo::Repo;
use crate::term::Cmd;
use crate::{exec, utils};

struct StepContext<'a> {
    env_readonly: HashMap<&'a str, &'a str>,
    env_mut: &'a mut HashMap<String, String>,

    cfg: &'a WorkflowStep,

    path: &'a PathBuf,

    docker: &'a Docker,

    display: bool,

    op: StepOperation<'a>,
}

enum StepOperation<'a> {
    Run(&'a str),
    Ssh(&'a str, &'a str),

    DockerRun(&'a str, &'a str),
    DockerPush(&'a str),
    DockerBuild(&'a str, &'a str),

    SetEnv(&'a str, &'a str),

    File(&'a str),
}

impl StepOperation<'_> {
    fn build(cfg: &WorkflowStep) -> Result<StepOperation<'_>> {
        if let Some(image) = cfg.image.as_ref() {
            let run = Self::must_get_run(cfg)?;
            return Ok(StepOperation::DockerRun(image, run));
        }

        if let Some(ssh) = cfg.ssh.as_ref() {
            let run = Self::must_get_run(cfg)?;
            return Ok(StepOperation::Ssh(ssh, run));
        }

        if let Some(set_env) = cfg.set_env.as_ref() {
            return Ok(StepOperation::SetEnv(&set_env.name, &set_env.value));
        }

        if let Some(docker_build) = cfg.docker_build.as_ref() {
            return Ok(StepOperation::DockerBuild(
                &docker_build.file,
                &docker_build.image,
            ));
        }

        if let Some(image) = cfg.docker_push.as_ref() {
            return Ok(StepOperation::DockerPush(image));
        }

        if let Some(file) = cfg.file.as_ref() {
            return Ok(StepOperation::File(file));
        }

        if let Some(run) = cfg.run.as_ref() {
            return Ok(StepOperation::Run(run));
        }

        bail!("invalid step '{}', missing operation", cfg.name)
    }

    fn must_get_run(cfg: &WorkflowStep) -> Result<&str> {
        match cfg.run.as_ref() {
            Some(run) => Ok(run),
            None => bail!("the run for step '{}' must be provided", cfg.name),
        }
    }
}

enum StepResult {
    Cmd(Cmd),

    File,
    SetEnv,

    Skip(&'static str),
}

impl StepContext<'_> {
    fn run(&mut self) -> Result<StepResult> {
        if let Some(os) = self.cfg.os.as_ref() {
            if !self.check_os(os)? {
                return Ok(StepResult::Skip("os mismatched"));
            }
        }

        if let Some(if_cond) = self.cfg.if_condition.as_ref() {
            if !self.check_if(if_cond)? {
                return Ok(StepResult::Skip("if condition did not pass"));
            }
        }

        if !self.cfg.condition.is_empty() {
            let if_cond = self.parse_condition(&self.cfg.condition);
            if !self.check_if(if_cond)? {
                return Ok(StepResult::Skip("condition did not pass"));
            }
        }

        match self.op {
            StepOperation::Run(run) => Ok(StepResult::Cmd(Cmd::sh(run, self.display))),
            StepOperation::Ssh(ssh, run) => {
                let args = ["ssh", ssh, run];
                Ok(StepResult::Cmd(Cmd::sh(args.join(" "), self.display)))
            }
            StepOperation::DockerRun(image, run) => {
                Ok(StepResult::Cmd(self.build_docker_run(image, run)?))
            }
            StepOperation::DockerPush(image) => {
                let image = self.expandenv(image)?;
                let args = vec!["push", image.as_ref()];
                Ok(StepResult::Cmd(self.build_docker_cmd(&args)))
            }
            StepOperation::DockerBuild(file, image) => {
                let file = self.expandenv(file)?;
                let image = self.expandenv(image)?;
                let args = vec!["build", "-f", file.as_ref(), "-t", image.as_ref(), "."];
                Ok(StepResult::Cmd(self.build_docker_cmd(&args)))
            }
            StepOperation::SetEnv(key, value) => {
                let value = self.expandenv(value)?;
                self.env_mut.insert(key.to_string(), value.into_owned());
                Ok(StepResult::SetEnv)
            }
            StepOperation::File(content) => {
                let path = self.path.join(&self.cfg.name);
                let content = content.replace("\\t", "\t");
                utils::write_file(&path, content.as_bytes())?;
                Ok(StepResult::File)
            }
        }
    }

    fn check_if(&self, cond: impl AsRef<str>) -> Result<bool> {
        let cmd = format!("if {}; then echo true; fi", cond.as_ref());
        let mut cmd = self.setup_cmd(Cmd::sh(cmd, self.display));

        let result = cmd.read().context("check if condition")?;
        Ok(result.trim() == "true")
    }

    fn parse_condition(&self, condition: &[WorkflowCondition]) -> String {
        let mut conds: Vec<String> = Vec::with_capacity(condition.len());
        for cond in condition.iter() {
            let cond = if let Some(env) = cond.env.as_ref() {
                if cond.exists {
                    format!("[[ -n \"${{{env}}}\" ]]")
                } else {
                    format!("[[ -z \"${{{env}}}\" ]]")
                }
            } else if let Some(file) = cond.file.as_ref() {
                if cond.exists {
                    format!("[[ -f {file} ]]")
                } else {
                    format!("[[ ! -f {file} ]]")
                }
            } else if let Some(cmd) = cond.cmd.as_ref() {
                if cond.exists {
                    format!("command -v {cmd}")
                } else {
                    format!("! command -v {cmd}")
                }
            } else {
                continue;
            };
            conds.push(cond);
        }
        conds.join(" && ")
    }

    fn build_docker_run(&self, image: &str, run: &str) -> Result<Cmd> {
        let mut args: Vec<Cow<str>> = Vec::new();
        args.push(Cow::Borrowed("run"));

        let mut envs: Vec<(&str, &str)> =
            Vec::with_capacity(self.env_readonly.len() + self.env_mut.len());
        for (key, value) in self.env_readonly.iter() {
            envs.push((key, value));
        }
        for (key, value) in self.env_mut.iter() {
            envs.push((key, value));
        }
        envs.sort_unstable_by(|(key0, _), (key1, _)| key0.cmp(key1));

        for (key, value) in envs {
            let env = format!("{key}={value}");
            args.push(Cow::Borrowed("-e"));
            args.push(Cow::Owned(env));
        }

        args.push(Cow::Borrowed("--entrypoint"));
        args.push(Cow::Borrowed(&self.docker.shell));

        args.push(Cow::Borrowed("-w"));
        args.push(Cow::Borrowed(&self.cfg.work_dir));

        args.push(Cow::Borrowed("-v"));
        let vol = format!("{}:{}", self.path.display(), self.cfg.work_dir);
        args.push(Cow::Borrowed(vol.as_str()));

        args.push(Cow::Borrowed("--rm"));
        args.push(Cow::Borrowed("-it"));

        args.push(self.expandenv(image)?);

        args.push(Cow::Borrowed("-c"));
        args.push(Cow::Borrowed(run));

        let mut cmd = self.build_docker_cmd(&args);
        cmd.display_docker(image.to_string(), run.to_string());
        Ok(cmd)
    }

    fn build_docker_cmd<S: AsRef<str>>(&self, args: &[S]) -> Cmd {
        let mut cmd_args: Vec<&str> = Vec::with_capacity(self.docker.args.len() + args.len());
        for arg in self.docker.args.iter() {
            cmd_args.push(arg);
        }
        for arg in args {
            cmd_args.push(arg.as_ref());
        }
        Cmd::with_args(&self.docker.name, &cmd_args)
    }

    fn setup_cmd(&self, cmd: Cmd) -> Cmd {
        let mut cmd = if self.display {
            cmd.with_display_cmd()
        } else {
            cmd
        };
        cmd.with_path(self.path);
        for (key, value) in self.env_readonly.iter() {
            cmd.with_env(key, value);
        }
        for (key, value) in self.env_mut.iter() {
            cmd.with_env(key, value);
        }
        cmd
    }

    fn expandenv<'a>(&self, s: &'a str) -> Result<Cow<'a, str>> {
        use std::prelude::v1::Result as StdResult;
        let match_env = |key: &_| -> StdResult<Option<Cow<str>>, &'static str> {
            if let Some(value) = self.env_mut.get(key) {
                return Ok(Some(Cow::Borrowed(value)));
            }
            if let Some(value) = self.env_readonly.get(key) {
                return Ok(Some(Cow::Borrowed(value)));
            }
            Err("env not found")
        };
        let value: Cow<str> = match shellexpand::env_with_context(s, match_env) {
            Ok(value) => value,
            Err(err) => bail!("expand env for '{s}': {err}"),
        };
        Ok(value)
    }

    fn check_os(&self, os: &WorkflowOS) -> Result<bool> {
        let current_os = Cmd::with_args("uname", &["-s"])
            .read()
            .context("use `uname -s` to check os")?
            .to_lowercase();
        match os {
            WorkflowOS::Linux => Ok(current_os == "linux"),
            WorkflowOS::Macos => Ok(current_os == "darwin"),
        }
    }
}

pub struct Workflow<C: AsRef<WorkflowConfig>> {
    path: PathBuf,
    cfg: C,

    env: HashMap<String, String>,
    step_env: Vec<HashMap<String, String>>,

    display: bool,

    docker: Docker,
}

impl<C: AsRef<WorkflowConfig>> Task<()> for Workflow<C> {
    fn run(&self) -> Result<()> {
        let mut env_mut = HashMap::new();

        let mut ops = Vec::with_capacity(self.cfg.as_ref().steps.len());
        for step_cfg in self.cfg.as_ref().steps.iter() {
            let op = StepOperation::build(step_cfg)?;
            ops.push(op);
        }

        let step_len = self.cfg.as_ref().steps.len();

        for (idx, step_cfg) in self.cfg.as_ref().steps.iter().enumerate() {
            if self.display {
                exec!("{}", style(&step_cfg.name).bold().cyan());
            }

            let step_env = &self.step_env[idx];
            let mut env_readonly = HashMap::with_capacity(self.env.len() + step_env.len());
            for (key, value) in self.env.iter() {
                env_readonly.insert(key.as_str(), value.as_str());
            }
            for (key, value) in step_env.iter() {
                env_readonly.insert(key.as_str(), value.as_str());
            }

            let mut ctx = StepContext {
                env_readonly,
                env_mut: &mut env_mut,
                cfg: step_cfg,
                path: &self.path,
                display: self.display,
                docker: &self.docker,
                op: ops.remove(0),
            };
            let result = ctx.run()?;
            let msg: Cow<str> = match result {
                StepResult::Cmd(cmd) => self.run_cmd(&mut ctx, cmd)?,
                StepResult::SetEnv => Cow::Borrowed("Set env done"),
                StepResult::Skip(msg) => Cow::Owned(format!("Skip: {}", style(msg).bold())),
                StepResult::File => Cow::Borrowed("Write to file done"),
            };

            if self.display {
                if !msg.is_empty() {
                    info!("{}", msg);
                }
                if idx != step_len - 1 {
                    eprintln!();
                }
            }
        }

        Ok(())
    }
}

impl<C: AsRef<WorkflowConfig>> Workflow<C> {
    pub fn new(cfg: &Config, repo: &Repo, workflow: C, display: bool) -> Workflow<C> {
        let path = repo.get_path(cfg);
        let env = build_env(repo, &workflow.as_ref().env, &path);
        let step_env: Vec<_> = workflow
            .as_ref()
            .steps
            .iter()
            .map(|step_cfg| build_env(repo, &step_cfg.env, &path))
            .collect();
        let docker = cfg.docker.clone();

        Workflow {
            path,
            cfg: workflow,
            env,
            step_env,
            display,
            docker,
        }
    }

    fn run_cmd(&self, ctx: &mut StepContext, cmd: Cmd) -> Result<Cow<str>> {
        let capture_output = ctx.cfg.capture_output.clone();

        let mut cmd = ctx.setup_cmd(cmd);
        let result = (|| -> Result<Cow<str>> {
            match capture_output {
                Some(env_name) => {
                    let output = cmd.read()?;
                    let msg = format!("Capture the command output to env '{}'", env_name.as_str());
                    ctx.env_mut.insert(env_name, output);
                    Ok(Cow::Owned(msg))
                }
                None => {
                    cmd.execute()?;
                    Ok(Cow::Borrowed(""))
                }
            }
        })();

        if result.is_err() && ctx.cfg.allow_failure {
            return Ok(Cow::Borrowed("Ignore step failure"));
        }

        result
    }
}

fn build_env(repo: &Repo, env_cfg: &[WorkflowEnv], path: &Path) -> HashMap<String, String> {
    let mut map = HashMap::with_capacity(env_cfg.len());
    for env in env_cfg.iter() {
        let key = env.name.clone();
        let mut value = env.value.clone();
        if let Some(from_repo) = env.from_repo.as_ref() {
            let from_repo = match from_repo {
                WorkflowFromRepo::Name => repo.name.to_string(),
                WorkflowFromRepo::Owner => repo.owner.to_string(),
                WorkflowFromRepo::Remote => repo.remote.to_string(),
                WorkflowFromRepo::Clone => match repo.remote_cfg.clone.as_ref().cloned() {
                    Some(clone) => clone.clone(),
                    None => String::new(),
                },
                WorkflowFromRepo::Path => format!("{}", path.display()),
            };
            let from_repo = if from_repo.is_empty() {
                match env.value.as_ref() {
                    Some(value) => value.clone(),
                    None => from_repo,
                }
            } else {
                from_repo
            };
            value = Some(from_repo);
        }
        let value = value.unwrap_or(String::new());
        map.insert(key, value);
    }
    map
}

impl Workflow<Arc<WorkflowConfig>> {
    pub fn load_for_batch(cfg: &Config, repo: &Repo, workflow: Arc<WorkflowConfig>) -> Self {
        Workflow::new(cfg, repo, workflow, false)
    }
}

impl<'a> Workflow<Cow<'a, WorkflowConfig>> {
    pub fn load(name: impl AsRef<str>, cfg: &'a Config, repo: &Repo) -> Result<Self> {
        let workflow = cfg.get_workflow(name.as_ref())?;
        Ok(Workflow::new(cfg, repo, workflow, true))
    }
}

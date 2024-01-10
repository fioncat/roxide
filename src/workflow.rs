use std::borrow::Cow;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};

use crate::batch::Task;
use crate::config::{
    Config, Docker, WorkflowCondition, WorkflowConfig, WorkflowEnv, WorkflowFromRepo, WorkflowOS,
    WorkflowStep,
};
use crate::repo::Repo;
use crate::term::Cmd;
use crate::{exec, info, stderrln, utils};

pub struct Workflow<C: AsRef<WorkflowConfig>> {
    pub name: String,
    pub cfg: C,
    pub path: PathBuf,

    env: Env,

    docker: Docker,

    display: Option<String>,
}

struct Env {
    global: HashMap<String, String>,

    steps: HashMap<usize, HashMap<String, String>>,
}

impl Env {
    fn build(repo: &Repo, cfg: &WorkflowConfig, path: &PathBuf) -> Env {
        let global = Self::build_map(repo, &cfg.env, path);
        let mut steps = HashMap::with_capacity(cfg.steps.len());
        for (idx, step) in cfg.steps.iter().enumerate() {
            if step.env.is_empty() {
                continue;
            }
            let map = Self::build_map(repo, &step.env, path);
            steps.insert(idx, map);
        }

        Env { global, steps }
    }

    fn build_map(repo: &Repo, vec: &Vec<WorkflowEnv>, path: &PathBuf) -> HashMap<String, String> {
        let mut map = HashMap::with_capacity(vec.len());
        for env in vec.iter() {
            let key = env.name.clone();
            let mut value = env.value.clone();
            if let Some(from_repo) = env.from_repo.as_ref() {
                let from_repo = match from_repo {
                    WorkflowFromRepo::Name => repo.name.to_string(),
                    WorkflowFromRepo::Owner => repo.owner.to_string(),
                    WorkflowFromRepo::Remote => repo.remote.to_string(),
                    WorkflowFromRepo::Clone => {
                        match repo.remote_cfg.clone.as_ref().map(|u| u.clone()) {
                            Some(clone) => clone.clone(),
                            None => String::new(),
                        }
                    }
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
            let value = value.unwrap_or_else(|| String::new());
            map.insert(key, value);
        }
        map
    }
}

impl<C: AsRef<WorkflowConfig>> Task<()> for Workflow<C> {
    fn run(&self) -> Result<()> {
        let mut extra_env = HashMap::new();

        if let Some(hint) = self.display.as_ref() {
            exec!("{}", hint);
            stderrln!();
        }
        let step_len = self.cfg.as_ref().steps.len();
        for (idx, step) in self.cfg.as_ref().steps.iter().enumerate() {
            if let Some(_) = self.display {
                exec!("Step '{}'", step.name);
            }
            self.run_step(idx, step, &mut extra_env)
                .with_context(|| format!("run step '{}' failed", step.name))?;
            if let Some(_) = self.display {
                if idx != step_len - 1 {
                    stderrln!();
                }
            }
        }
        Ok(())
    }
}

impl<C: AsRef<WorkflowConfig>> Workflow<C> {
    pub fn new(
        cfg: &Config,
        repo: &Repo,
        name: impl AsRef<str>,
        wf_cfg: C,
        mute: bool,
    ) -> Workflow<C> {
        let path = repo.get_path(cfg);
        let env = Env::build(repo, wf_cfg.as_ref(), &path);
        let display = if mute {
            None
        } else {
            Some(format!(
                "Run workflow '{}' for '{}'",
                name.as_ref(),
                repo.name_with_remote()
            ))
        };

        Workflow {
            name: name.as_ref().to_string(),
            cfg: wf_cfg,
            path,
            docker: cfg.docker.clone(),
            env,
            display,
        }
    }

    fn run_step(
        &self,
        idx: usize,
        step: &WorkflowStep,
        extra_env: &mut HashMap<String, String>,
    ) -> Result<()> {
        if let Some(os) = step.os.as_ref() {
            if !self.check_os(os)? {
                self.show_step_info("Skip because of mismatched os");
                return Ok(());
            }
        }

        if let Some(if_condition) = step.if_condition.as_ref() {
            if !self.check_if(idx, &extra_env, if_condition)? {
                self.show_step_info("Skip because the if condition check did not pass");
                return Ok(());
            }
        }

        if !step.condition.is_empty() {
            let if_condition = self.parse_condition(&step.condition);
            if !self.check_if(idx, &extra_env, if_condition)? {
                self.show_step_info("Skip because the condition check did not pass");
                return Ok(());
            }
        }

        if let Some(content) = step.file.as_ref() {
            self.show_step_info(format!(
                "Write file with {} data",
                utils::human_bytes(content.len() as u64)
            ));
            let path = self.path.join(&step.name);
            utils::write_file(&path, content.as_bytes())?;
            return Ok(());
        }

        let mut capture_output = step.capture_output.as_ref().map(|s| s.clone());
        let mut cmd = if let Some(_) = step.image {
            self.docker_cmd(idx, step, &extra_env)
        } else if let Some(_) = step.ssh {
            self.ssh_cmd(step)
        } else if let Some(set_env) = step.set_env.as_ref() {
            let cmd = format!("echo \"{}\"", set_env.value);
            capture_output = Some(set_env.name.clone());
            Cmd::sh(cmd)
        } else if let Some(image) = step.docker_push.as_ref() {
            let cmd = format!("{} push {}", self.docker.cmd, image);
            Cmd::sh(cmd)
        } else if let Some(docker_build) = step.docker_build.as_ref() {
            let file = docker_build
                .file
                .as_ref()
                .map(|s| s.as_str())
                .unwrap_or("Dockerfile");
            let cmd = format!(
                "{} build -f {} -t {} .",
                self.docker.cmd, file, docker_build.image
            );
            Cmd::sh(cmd)
        } else {
            match step.run.as_ref() {
                Some(run) => Cmd::sh(run),
                None => return Ok(()),
            }
        };
        self.setup_env(idx, &mut cmd, extra_env);

        if let Some(_) = self.display.as_ref() {
            cmd = cmd.with_display_cmd();
        }

        cmd.with_path(&self.path);
        let result = (|| -> Result<()> {
            match capture_output {
                Some(env_name) => {
                    let output = cmd.read()?;
                    self.show_step_info(format!(
                        "Capture the command output to env '{}'",
                        env_name.as_str()
                    ));
                    extra_env.insert(env_name, output);
                    Ok(())
                }
                None => cmd.execute(),
            }
        })();
        if let Err(_) = result {
            if step.allow_failure {
                self.show_step_info("Ignore step failure because of `allow_failure`");
                return Ok(());
            }
        }
        result
    }

    #[inline]
    fn show_step_info(&self, s: impl AsRef<str>) {
        if let Some(_) = self.display {
            info!("{}", s.as_ref());
        }
    }

    fn docker_cmd(
        &self,
        idx: usize,
        step: &WorkflowStep,
        extra_env: &HashMap<String, String>,
    ) -> Cmd {
        let mut args: Vec<Cow<str>> = Vec::new();
        args.push(Cow::Borrowed(self.docker.cmd.as_str()));
        args.push(Cow::Borrowed("run"));

        let mut append_env = |env_map: &HashMap<String, String>| {
            let envs: Vec<String> = env_map.iter().map(|(k, v)| format!("{k}={v}")).collect();
            for env in envs {
                args.push(Cow::Borrowed("--env"));
                args.push(Cow::Owned(env));
            }
        };

        append_env(&self.env.global);
        append_env(&extra_env);
        if let Some(step_env) = self.env.steps.get(&idx) {
            append_env(step_env);
        }

        args.push(Cow::Borrowed("--entrypoint"));
        args.push(Cow::Borrowed(&self.docker.shell));

        let workdir = match step.work_dir.as_ref() {
            Some(dir) => dir.as_str(),
            None => "/work",
        };
        args.push(Cow::Borrowed("--workdir"));
        args.push(Cow::Borrowed(workdir));

        args.push(Cow::Borrowed("--volume"));
        let vol = format!("{}:{}", self.path.display(), workdir);
        args.push(Cow::Borrowed(vol.as_str()));

        args.push(Cow::Borrowed("--rm"));
        args.push(Cow::Borrowed("-it"));

        args.push(Cow::Borrowed(step.image.as_ref().unwrap().as_str()));

        args.push(Cow::Borrowed("-c"));
        let script = step.run.as_ref().unwrap().as_str().replace("'", "\\'");
        let script = format!("'{script}'");
        args.push(Cow::Owned(script));

        Cmd::sh(args.join(" ").as_str())
    }

    fn ssh_cmd(&self, step: &WorkflowStep) -> Cmd {
        let mut args = Vec::new();

        args.push("ssh");
        args.push(step.ssh.as_ref().unwrap().as_str());
        args.push(step.run.as_ref().unwrap().as_str());

        Cmd::sh(args.join(" ").as_str())
    }

    fn setup_env(&self, idx: usize, cmd: &mut Cmd, extra_env: &HashMap<String, String>) {
        for (k, v) in extra_env.iter() {
            cmd.with_env(k, v);
        }
        for (k, v) in self.env.global.iter() {
            cmd.with_env(k, v);
        }
        if let Some(envs) = self.env.steps.get(&idx) {
            for (k, v) in envs.iter() {
                cmd.with_env(k, v);
            }
        }
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

    fn check_if(
        &self,
        idx: usize,
        extra_env: &HashMap<String, String>,
        condition: impl AsRef<str>,
    ) -> Result<bool> {
        let cmd = format!("if {}; then echo true; fi", condition.as_ref());
        let mut cmd = Cmd::sh(cmd);

        if let Some(_) = self.display {
            cmd = cmd.with_display_cmd();
        }

        self.setup_env(idx, &mut cmd, extra_env);

        let result = cmd.read().context("check if condition")?;
        Ok(result.trim() == "true")
    }

    fn parse_condition(&self, condition: &Vec<WorkflowCondition>) -> String {
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
}

impl Workflow<Arc<WorkflowConfig>> {
    pub fn load_for_batch(
        cfg: &Config,
        repo: &Repo,
        name: impl AsRef<str>,
        wf_cfg: Arc<WorkflowConfig>,
    ) -> Self {
        Workflow::new(cfg, repo, name, wf_cfg, true)
    }
}

impl<'a> Workflow<Cow<'a, WorkflowConfig>> {
    pub fn load(cfg: &'a Config, repo: &Repo, name: impl AsRef<str>) -> Result<Self> {
        let wf_cfg = cfg.get_workflow(name.as_ref())?;
        Ok(Workflow::new(cfg, repo, name, wf_cfg, false))
    }
}

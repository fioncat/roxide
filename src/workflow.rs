use std::borrow::Cow;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use anyhow::{bail, Context, Result};

use crate::batch::Task;
use crate::config::{Config, Docker, WorkflowConfig, WorkflowEnv, WorkflowFromRepo, WorkflowStep};
use crate::repo::Repo;
use crate::term::Cmd;
use crate::{exec, info, utils};

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
    fn build(repo: &Rc<Repo>, cfg: &WorkflowConfig, path: &PathBuf) -> Env {
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

    fn build_map(
        repo: &Rc<Repo>,
        vec: &Vec<WorkflowEnv>,
        path: &PathBuf,
    ) -> HashMap<String, String> {
        let mut map = HashMap::with_capacity(vec.len());
        for env in vec.iter() {
            let key = env.name.clone();
            let mut value = env.value.clone();
            if let Some(from_repo) = env.from_repo.as_ref() {
                let from_repo = match from_repo {
                    WorkflowFromRepo::Name => repo.name.clone(),
                    WorkflowFromRepo::Owner => repo.owner.name.to_string(),
                    WorkflowFromRepo::Remote => repo.remote.name.to_string(),
                    WorkflowFromRepo::Clone => match repo.remote.cfg.clone.as_ref() {
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
            let value = match value {
                Some(value) => value,
                None => String::new(),
            };
            map.insert(key, value);
        }
        map
    }
}

impl<C: AsRef<WorkflowConfig>> Task<()> for Workflow<C> {
    fn run(&self) -> Result<()> {
        let mut global_env = self.env.global.clone();

        if let Some(hint) = self.display.as_ref() {
            exec!("{}", hint);
        }
        for (idx, step) in self.cfg.as_ref().steps.iter().enumerate() {
            self.run_step(idx, step, &mut global_env)
                .with_context(|| format!("run step '{}' failed", step.name))?;
        }
        Ok(())
    }
}

impl<C: AsRef<WorkflowConfig>> Workflow<C> {
    fn new(
        cfg: &Config,
        repo: &Rc<Repo>,
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
        if let Some(content) = step.file.as_ref() {
            if let Some(_) = self.display.as_ref() {
                exec!(
                    "Write file '{}', with {} data",
                    step.name,
                    utils::human_bytes(content.len() as u64)
                );
            }
            let path = self.path.join(&step.name);
            utils::write_file(&path, content.as_bytes())?;
            return Ok(());
        }
        if let None = step.run {
            return Ok(());
        }

        let mut cmd = if let Some(_) = step.image {
            self.docker_cmd(idx, step, &extra_env)
        } else if let Some(_) = step.ssh {
            self.ssh_cmd(step)
        } else {
            Cmd::sh(step.run.as_ref().unwrap().as_str(), step.is_capture())
        };
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

        if let Some(_) = self.display.as_ref() {
            let hint = format!("Run workflow step '{}'", step.name);
            cmd = cmd.with_display(hint);
        }

        cmd.with_path(&self.path);
        if step.is_capture() {
            let env_name = step.capture_output.as_ref().unwrap().clone();
            let output = cmd.read()?;
            info!("Capture the command output to env '{}'", env_name.as_str());
            extra_env.insert(env_name, output);
            Ok(())
        } else {
            cmd.execute_check()
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

        Cmd::sh(args.join(" ").as_str(), step.is_capture())
    }

    fn ssh_cmd(&self, step: &WorkflowStep) -> Cmd {
        let mut args = Vec::new();

        args.push("ssh");
        args.push(step.ssh.as_ref().unwrap().as_str());
        args.push(step.run.as_ref().unwrap().as_str());

        Cmd::sh(args.join(" ").as_str(), step.is_capture())
    }
}

impl Workflow<Arc<WorkflowConfig>> {
    pub fn load_for_batch(
        cfg: &Config,
        repo: &Rc<Repo>,
        name: impl AsRef<str>,
        wf_cfg: Arc<WorkflowConfig>,
    ) -> Workflow<Arc<WorkflowConfig>> {
        Workflow::new(cfg, repo, name, wf_cfg, true)
    }
}

impl Workflow<Rc<WorkflowConfig>> {
    pub fn load(
        cfg: &Config,
        repo: &Rc<Repo>,
        name: impl AsRef<str>,
    ) -> Result<Workflow<Box<WorkflowConfig>>> {
        let wf_cfg = match cfg.workflows.get(name.as_ref()) {
            Some(wf_cfg) => wf_cfg.clone(),
            None => bail!("could not find workflow '{}'", name.as_ref()),
        };
        let wf_cfg = Box::new(wf_cfg);

        Ok(Workflow::new(cfg, repo, name, wf_cfg, false))
    }
}

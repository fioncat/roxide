use std::collections::HashMap;
use std::path::PathBuf;

use crate::config::{WorkflowConfig, WorkflowEnv, WorkflowFromRepo, WorkflowStep};

pub struct Workflow {
    pub name: String,
    pub cfg: WorkflowConfig,

    env: Env,

    mute: bool,

    display: Option<String>,
}

pub struct WorkDir<'a> {
    pub path: PathBuf,

    pub name: &'a str,
    pub owner: &'a str,
    pub remote: &'a str,

    pub clone: &'a str,
}

struct Env {
    global: HashMap<String, String>,

    steps: HashMap<usize, HashMap<String, String>>,
}

impl Env {
    fn build(dir: &WorkDir, cfg: &WorkflowConfig) -> Env {
        let global = Self::build_map(dir, &cfg.env);
        let mut steps = HashMap::with_capacity(cfg.steps.len());
        for (idx, step) in cfg.steps.iter().enumerate() {
            if step.env.is_empty() {
                continue;
            }
            let map = Self::build_map(dir, &step.env);
            steps.insert(idx, map);
        }

        Env { global, steps }
    }

    fn build_map(dir: &WorkDir, vec: &Vec<WorkflowEnv>) -> HashMap<String, String> {
        let mut map = HashMap::with_capacity(vec.len());
        for env in vec.iter() {
            let key = env.name.clone();
            let mut value = env.value.clone();
            if let Some(from_repo) = env.from_repo.as_ref() {
                let from_repo = match from_repo {
                    WorkflowFromRepo::Name => dir.name.to_string(),
                    WorkflowFromRepo::Owner => dir.owner.to_string(),
                    WorkflowFromRepo::Remote => dir.remote.to_string(),
                    WorkflowFromRepo::Clone => dir.clone.to_string(),
                    WorkflowFromRepo::Path => format!("{}", dir.path.display()),
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

impl Workflow {}

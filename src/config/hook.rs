use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::debug;
use crate::repo::hook::condition::Condition;
use crate::repo::hook::filter::Filter;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HookConfig {
    pub name: String,

    #[serde(default)]
    pub when: Vec<String>,

    #[serde(default)]
    pub on: Vec<String>,

    pub run: Vec<String>,

    #[serde(skip)]
    pub conditions: Vec<Condition>,

    #[serde(skip)]
    pub filters: Vec<Filter>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HookRuns {
    pub hooks: HashMap<String, String>,
}

impl HookConfig {
    pub fn validate_hooks(hooks: &mut [HookConfig], runs: &HookRuns) -> Result<()> {
        debug!("[config] Validating hooks config: {hooks:?}");
        let mut names = HashSet::new();
        for hook in hooks {
            if names.contains(&hook.name) {
                bail!("duplicate hook name: {:?}", hook.name);
            }
            names.insert(&hook.name);

            for run in hook.run.iter() {
                if !runs.contains(run) {
                    bail!("hook {:?} references unknown run: {run:?}", hook.name);
                }
            }

            let mut conditions = Vec::with_capacity(hook.when.len());
            for when in hook.when.iter() {
                let cond = Condition::parse(when)
                    .with_context(|| format!("failed to parse when clause {when:?}"))?;
                conditions.push(cond);
            }

            let mut filters = Vec::with_capacity(hook.on.len());
            for on in hook.on.iter() {
                let filter = Filter::parse(on)
                    .with_context(|| format!("failed to parse on clause {on:?}"))?;
                filters.push(filter);
            }

            hook.conditions = conditions;
            hook.filters = filters;
            debug!("[config] Validated hook config: {hook:?}");
        }
        Ok(())
    }
}

impl HookRuns {
    pub fn read(dir: &Path) -> Result<Self> {
        debug!("[config] Reading hooks config from {}", dir.display());
        let ents = match fs::read_dir(dir) {
            Ok(d) => {
                debug!("[config] Hooks dir found");
                d
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                debug!("[config] Hooks dir not found, returns empty");
                return Ok(Self::default());
            }
            Err(e) => {
                return Err(e)
                    .with_context(|| format!("failed to read hooks dir {}", dir.display()));
            }
        };

        let mut hooks = HashMap::new();
        for ent in ents {
            let ent = ent.context("read hooks dir entry")?;
            let file_name = ent.file_name();
            let file_name = match file_name.to_str() {
                Some(s) => s,
                None => continue,
            };
            if !file_name.ends_with(".sh") {
                continue;
            }
            let name = file_name.trim_end_matches(".sh").to_string();
            let path = PathBuf::from(dir).join(file_name);
            let path = format!("{}", path.display());
            debug!("[config] Found hook: {name}: {path}");
            hooks.insert(name, path);
        }

        Ok(Self { hooks })
    }

    pub fn contains(&self, name: &str) -> bool {
        self.hooks.contains_key(name)
    }

    pub fn get<'a>(&'a self, name: &str) -> Option<&'a String> {
        self.hooks.get(name)
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    pub fn expect_hooks() -> Vec<HookConfig> {
        vec![
            HookConfig {
                name: "cargo-init".to_string(),
                when: vec!["created".to_string()],
                on: vec!["test rust".to_string()],
                run: vec!["cargo-init".to_string()],
                conditions: vec![Condition::Created],
                filters: vec![Filter::parse("test rust").unwrap()],
            },
            HookConfig {
                name: "gomod-init".to_string(),
                when: vec!["created".to_string()],
                on: vec!["test golang".to_string()],
                run: vec!["gomod-init".to_string()],
                conditions: vec![Condition::Created],
                filters: vec![Filter::parse("test golang").unwrap()],
            },
            HookConfig {
                name: "print-envs".to_string(),
                run: vec!["print-envs".to_string()],
                ..Default::default()
            },
        ]
    }

    pub fn expect_hook_runs() -> HookRuns {
        let dir = "src/config/tests/hooks";
        let dir = fs::canonicalize(dir).unwrap();
        let dir = format!("{}", dir.display());
        let mut expected = HashMap::new();
        expected.insert("cargo-init".to_string(), format!("{dir}/cargo-init.sh"));
        expected.insert("gomod-init".to_string(), format!("{dir}/gomod-init.sh"));
        expected.insert("print-envs".to_string(), format!("{dir}/print-envs.sh"));
        HookRuns { hooks: expected }
    }

    #[test]
    fn test_hooks_config() {
        let mut hooks = vec![
            HookConfig {
                name: "cargo-init".to_string(),
                when: vec!["created".to_string()],
                on: vec!["test rust".to_string()],
                run: vec!["cargo-init".to_string()],
                conditions: vec![],
                filters: vec![],
            },
            HookConfig {
                name: "gomod-init".to_string(),
                when: vec!["created".to_string()],
                on: vec!["test golang".to_string()],
                run: vec!["gomod-init".to_string()],
                conditions: vec![],
                filters: vec![],
            },
            HookConfig {
                name: "print-envs".to_string(),
                run: vec!["print-envs".to_string()],
                ..Default::default()
            },
        ];
        let runs = expect_hook_runs();
        HookConfig::validate_hooks(&mut hooks, &runs).unwrap();
        let expect = expect_hooks();
        assert_eq!(hooks, expect);
    }

    #[test]
    fn test_hook_runs_config() {
        let dir = "src/config/tests/hooks";
        let dir = fs::canonicalize(dir).unwrap();
        let hooks = HookRuns::read(&dir).unwrap();
        assert_eq!(hooks, expect_hook_runs());
    }

    #[test]
    fn test_default_hook_runs() {
        let dir = "src/config"; // no .sh files here
        let hooks = HookRuns::read(Path::new(dir)).unwrap();
        assert_eq!(hooks, HookRuns::default());
    }
}

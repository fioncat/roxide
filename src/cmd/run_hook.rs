use std::collections::{HashMap, HashSet};
use std::mem::take;
use std::sync::Arc;

use anyhow::{Result, bail};
use async_trait::async_trait;
use clap::Args;
use console::style;

use crate::batch::{self, Task};
use crate::cmd::ThinArgs;
use crate::cmd::complete::{CompleteArg, CompleteCommand};
use crate::config::context::ConfigContext;
use crate::config::hook::HookConfig;
use crate::db::repo::Repository;
use crate::repo::current::get_current_repo_optional;
use crate::repo::ops::{CreateResult, RepoOperator};
use crate::repo::select::{RepoSelector, SelectManyReposOptions, SelectRepoArgs};
use crate::term::confirm::confirm_items;
use crate::{debug, outputln};

use super::Command;

/// Manually run hooks for one or multiple repositories.
#[derive(Debug, Args)]
pub struct RunHookCommand {
    #[clap(flatten)]
    pub select_repo: SelectRepoArgs,

    /// The hook names to run. If not specified, run only matched hooks
    #[arg(short)]
    pub names: Option<Vec<String>>,

    /// Run hooks for multiple repositories.
    #[arg(short)]
    pub recursive: bool,

    #[clap(flatten)]
    pub thin: ThinArgs,
}

#[async_trait]
impl Command for RunHookCommand {
    fn name() -> &'static str {
        "run-hook"
    }

    async fn run(self, ctx: ConfigContext) -> Result<()> {
        debug!("[cmd] Run run command: {:?}", self);

        if !self.recursive
            && let Some(repo) = get_current_repo_optional(&ctx)?
        {
            return self.run_one(ctx, repo);
        }
        self.run_many(ctx).await
    }

    fn complete() -> CompleteCommand {
        Self::default_complete()
            .args(SelectRepoArgs::complete())
            // TODO: Support complete hook names
            .arg(CompleteArg::new().short('n').no_complete_value())
            .arg(CompleteArg::new().short('r'))
            .arg(ThinArgs::complete())
    }
}

impl RunHookCommand {
    async fn run_many(self, mut ctx: ConfigContext) -> Result<()> {
        let selector = RepoSelector::new(&ctx, &self.select_repo);
        let list = selector.select_many(SelectManyReposOptions::default())?;
        if list.items.is_empty() {
            outputln!("No repo to run");
            return Ok(());
        }

        let hooks = if let Some(names) = self.names {
            if names.is_empty() {
                bail!("hook names cannot be empty");
            }
            let mut hooks = Vec::with_capacity(names.len());
            let mut names_set: HashSet<String> = HashSet::new();
            for name in names {
                if names_set.contains(&name) {
                    continue;
                }
                let hook = ctx.cfg.get_hook(&name)?.clone();
                hooks.push(Arc::new(hook));
                names_set.insert(name);
            }
            Some(hooks)
        } else {
            None
        };

        let all_hooks = ctx
            .cfg
            .hooks
            .iter()
            .map(|hook| (hook.name.clone(), Arc::new(hook.clone())))
            .collect::<HashMap<_, _>>();
        let all_hooks = Arc::new(all_hooks);

        let mut names = list.display_names();
        confirm_items(&names, "Run", "run", "Repo", "Repos")?;

        let mut tasks = Vec::with_capacity(list.items.len());
        ctx.mute();
        let ctx = Arc::new(ctx);
        for (idx, repo) in list.items.into_iter().enumerate() {
            let task = RunTask {
                name: Arc::new(take(&mut names[idx])),
                ctx: ctx.clone(),
                hooks: hooks.clone(),
                all_hooks: all_hooks.clone(),
                repo,
                thin: self.thin.enable,
            };
            tasks.push(task);
        }

        let results = batch::run("Running hook", "Hook", tasks).await?;
        let results = results
            .into_iter()
            .filter(|res| !res.results.is_empty())
            .collect::<Vec<_>>();
        outputln!();
        if results.is_empty() {
            outputln!("No result to display");
            return Ok(());
        }

        for (idx, result) in results.iter().enumerate() {
            outputln!("{}", result.name);
            for hook_result in result.results.iter() {
                let status = if hook_result.success {
                    style("ok").green()
                } else {
                    style("failed").red()
                };
                outputln!("  {} {status}", hook_result.hook.name);
            }
            if idx != results.len() - 1 {
                outputln!();
            }
        }

        Ok(())
    }

    fn run_one(self, ctx: ConfigContext, repo: Repository) -> Result<()> {
        let op = RepoOperator::load(&ctx, &repo)?;

        if let Some(names) = self.names {
            let mut hooks = Vec::with_capacity(names.len());
            for name in names {
                let hook = ctx.cfg.get_hook(&name)?;
                hooks.push(hook);
            }
            let envs = op.build_hook_envs();
            for hook in hooks {
                op.run_hook(hook, &envs)?;
            }

            return Ok(());
        }

        op.run_hooks(CreateResult::Exists)?;
        Ok(())
    }
}

struct RunTask {
    name: Arc<String>,
    ctx: Arc<ConfigContext>,
    hooks: Option<Vec<Arc<HookConfig>>>,
    all_hooks: Arc<HashMap<String, Arc<HookConfig>>>,
    repo: Repository,
    thin: bool,
}

struct RunResult {
    name: Arc<String>,
    results: Vec<HookResult>,
}

struct HookResult {
    hook: Arc<HookConfig>,
    success: bool,
}

#[async_trait]
impl Task<RunResult> for RunTask {
    fn name(&self) -> Arc<String> {
        self.name.clone()
    }

    async fn run(&self) -> Result<RunResult> {
        let op = RepoOperator::load(self.ctx.as_ref(), &self.repo)?;
        let create_result = op.ensure_create(self.thin, None)?;

        if let Some(ref hooks) = self.hooks {
            let mut results = Vec::new();
            let envs = op.build_hook_envs();
            for hook in hooks {
                let success = op.run_hook(hook.as_ref(), &envs)?;
                results.push(HookResult {
                    hook: hook.clone(),
                    success,
                });
            }
            return Ok(RunResult {
                name: self.name.clone(),
                results,
            });
        }

        let results = op.run_hooks(create_result)?;
        let results = results
            .iter()
            .map(|result| HookResult {
                hook: self.all_hooks.get(result.name).unwrap().clone(),
                success: result.success,
            })
            .collect::<Vec<_>>();
        Ok(RunResult {
            name: self.name.clone(),
            results,
        })
    }
}

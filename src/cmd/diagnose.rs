use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::{fs, io};

use anyhow::{Context, Result};
use clap::Args;
use console::style;
use regex::Regex;

use crate::batch::{self, Task};
use crate::cmd::{Completion, Run};
use crate::config::{Config, RemoteConfig};
use crate::repo::database::{Database, SelectOptions, Selector};
use crate::repo::{NameLevel, Repo};
use crate::term::{BranchStatus, GitBranch, GitCmd};
use crate::{hashset_strings, term, utils};

/// Check if branches need to be updated.
#[derive(Args)]
pub struct DiagnoseArgs {
    /// Repository selection head.
    pub head: Option<String>,

    /// Repository selection query.
    pub query: Option<String>,

    /// Use editor to filter items before sync.
    #[clap(short, long)]
    pub edit: bool,

    /// Force spy, ignore label "sync".
    #[clap(short, long)]
    pub force: bool,

    /// Use the labels to filter repository.
    #[clap(short, long)]
    pub labels: Option<String>,
}

impl Run for DiagnoseArgs {
    fn run(&self, cfg: &Config) -> Result<()> {
        let db = Database::load(cfg)?;

        let filter_labels = utils::parse_labels(&self.labels);
        let filter_labels = if self.force {
            filter_labels
        } else {
            match filter_labels {
                Some(labels) => {
                    let mut new_labels = HashSet::with_capacity(labels.len() + 1);
                    new_labels.insert(String::from("sync"));
                    new_labels.extend(labels);
                    Some(new_labels)
                }
                None => Some(hashset_strings!["sync"]),
            }
        };

        let opts = SelectOptions::default()
            .with_filter_labels(filter_labels)
            .with_many_edit(self.edit);
        let selector = Selector::from_args(&self.head, &self.query, opts);

        let (repos, level) = selector.many_local(&db)?;
        if repos.is_empty() {
            eprintln!("No repo to diagnose");
            return Ok(());
        }

        let items: Vec<String> = repos.iter().map(|repo| repo.to_string(&level)).collect();
        term::must_confirm_items(&items, "diagnose", "diagnosis", "Repo", "Repos")?;

        let tasks = self.build_tasks(cfg, repos, &level)?;

        let results = batch::must_run("Diagnose", tasks)?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        if results.is_empty() {
            eprintln!();
            eprintln!("Nothing to show, great!");
            return Ok(());
        }

        for result in results {
            eprintln!();
            result.show();
        }

        Ok(())
    }
}

impl DiagnoseArgs {
    fn build_tasks(
        &self,
        cfg: &Config,
        repos: Vec<Repo>,
        level: &NameLevel,
    ) -> Result<Vec<(String, DiagnoseTask)>> {
        let branch_re = Arc::new(GitBranch::get_regex());

        let mut tasks = Vec::with_capacity(repos.len());
        let mut remotes: HashMap<&str, Arc<RemoteConfig>> = HashMap::new();

        for repo in repos.iter() {
            let (remote, remote_cfg) = match remotes.remove_entry(repo.remote.as_ref()) {
                Some((remote, cfg)) => (remote, cfg),
                None => {
                    let remote_cfg = cfg.must_get_remote(repo.remote.as_ref())?;
                    let remote_cfg = Arc::new(remote_cfg.into_owned());
                    (repo.remote.as_ref(), remote_cfg)
                }
            };

            let task_remote_cfg = Arc::clone(&remote_cfg);
            remotes.insert(remote, remote_cfg);

            let path = repo.get_path(cfg);

            tasks.push((
                repo.to_string(level),
                DiagnoseTask {
                    remote_cfg: task_remote_cfg,
                    name: repo.to_string(level),
                    path,
                    branch_re: Arc::clone(&branch_re),
                },
            ));
        }
        Ok(tasks)
    }

    pub fn completion() -> Completion {
        Completion {
            args: Completion::repo_args,
            flags: Some(|cfg, flag, to_complete| match flag {
                'l' => Completion::labels_flag(cfg, to_complete),
                _ => Ok(None),
            }),
        }
    }
}

struct DiagnoseTask {
    remote_cfg: Arc<RemoteConfig>,
    name: String,

    path: PathBuf,

    branch_re: Arc<Regex>,
}

struct DiagnoseResult {
    name: String,
    missing: bool,
    branches: Vec<DiagnoseBranch>,
}

struct DiagnoseBranch {
    name: String,
    info_list: Vec<DiagnoseInfo>,
}

struct DiagnoseInfo {
    info: Cow<'static, str>,
    suggestion: &'static str,
}

impl Task<Option<DiagnoseResult>> for DiagnoseTask {
    fn run(&self) -> Result<Option<DiagnoseResult>> {
        if self.remote_cfg.clone.is_none() {
            return Ok(None);
        }

        let missing = match fs::read_dir(&self.path) {
            Ok(_) => false,
            Err(err) if err.kind() == io::ErrorKind::NotFound => true,
            Err(err) => {
                return Err(err).with_context(|| format!("read repo dir '{}'", self.path.display()))
            }
        };
        if missing {
            return Ok(Some(DiagnoseResult {
                name: self.name.clone(),
                missing: true,
                branches: vec![],
            }));
        }

        let path = format!("{}", self.path.display());
        let git = GitCmd::with_path(&path);
        let mut branches = vec![];

        git.exec(&["fetch", "origin", "--prune"])?;

        let lines = git.lines(&["remote", "show", "origin"])?;
        let default_branch = GitBranch::parse_default_branch(lines)?;

        let lines = git.lines(&["branch", "-vv"])?;
        for line in lines {
            let branch = GitBranch::parse(&self.branch_re, line.as_str())?;
            let mut info_list = vec![];
            if branch.current {
                let lines = git.lines(&["status", "-s"])?;
                if !lines.is_empty() {
                    info_list.push(DiagnoseInfo {
                        info: Cow::Owned(format!("{} uncommitted change(s)", lines.len())),
                        suggestion: "commit",
                    });
                }
            }

            let mut need_check_rebase = true;
            if branch.name == default_branch {
                need_check_rebase = false;
            }

            match branch.status {
                BranchStatus::Detached => {
                    info_list.push(DiagnoseInfo {
                        info: Cow::Borrowed("Missing in remote"),
                        suggestion: "create",
                    });
                    need_check_rebase = false;
                }
                BranchStatus::Gone => {
                    info_list.push(DiagnoseInfo {
                        info: Cow::Borrowed("Deleted in remote"),
                        suggestion: "delete",
                    });
                    need_check_rebase = false;
                }
                BranchStatus::Behind | BranchStatus::Ahead | BranchStatus::Conflict => {
                    let (ahead, behind) =
                        compare_logs(&git, branch.name.as_str(), default_branch.as_str())?;

                    if ahead > 0 {
                        info_list.push(DiagnoseInfo {
                            info: Cow::Owned(format!("{} commit(s) ahead remote", ahead)),
                            suggestion: "push",
                        });
                    }

                    if behind > 0 {
                        info_list.push(DiagnoseInfo {
                            info: Cow::Owned(format!("{} commit(s) behind remote", behind)),
                            suggestion: "pull",
                        });
                    }
                }
                BranchStatus::Sync => {}
            }

            if need_check_rebase {
                let (ahead, behind) =
                    compare_logs(&git, branch.name.as_str(), default_branch.as_str())?;
                if ahead > 0 {
                    info_list.push(DiagnoseInfo {
                        info: Cow::Owned(format!("{ahead} commit(s) ahead {default_branch}")),
                        suggestion: "merge",
                    });
                }
                if behind > 0 {
                    info_list.push(DiagnoseInfo {
                        info: Cow::Owned(format!("{behind} commit(s) behind {default_branch}")),
                        suggestion: "rebase",
                    });
                }
            }

            if !info_list.is_empty() {
                branches.push(DiagnoseBranch {
                    name: branch.name,
                    info_list,
                });
            }
        }

        if branches.is_empty() {
            return Ok(None);
        }

        Ok(Some(DiagnoseResult {
            name: self.name.clone(),
            missing: false,
            branches,
        }))
    }
}

impl DiagnoseResult {
    fn show(&self) {
        if self.missing {
            eprintln!(
                "{}: {}",
                style(&self.name).bold().cyan(),
                style("Missing").bold().red()
            );
            return;
        }

        eprintln!("{}:", style(&self.name).bold().cyan());
        for branch in self.branches.iter() {
            eprintln!("  * {}:", style(&branch.name).green());
            for info in branch.info_list.iter() {
                eprintln!(
                    "    > {} ({})",
                    info.info,
                    style(info.suggestion).magenta().bold()
                );
            }
        }
    }
}

fn compare_logs(git: &GitCmd, left: &str, right: &str) -> Result<(usize, usize)> {
    let compare = format!("{left}...origin/{right}");
    let log_lines = git.lines(&[
        "log",
        "--left-right",
        "--cherry-pick",
        "--oneline",
        compare.as_str(),
    ])?;
    let mut ahead: usize = 0;
    let mut behind: usize = 0;
    for log_line in log_lines {
        if log_line.starts_with('<') {
            ahead += 1;
        } else if log_line.starts_with('>') {
            behind += 1;
        }
    }
    Ok((ahead, behind))
}

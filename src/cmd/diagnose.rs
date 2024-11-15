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
use crate::exec::GitCmd;
use crate::git::{BranchStatus, GitBranch};
use crate::repo::database::{Database, SelectOptions, Selector};
use crate::repo::{NameLevel, Repo};
use crate::table::{Table, TableCell, TableCellColor};
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

    /// Force diagnose, ignore label "sync".
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
    default_branch: String,
    branches: Vec<DiagnoseBranch>,
}

struct DiagnoseBranch {
    name: String,

    uncommitted: usize,

    detached: bool,
    gone: bool,

    ahead_remote: usize,
    behind_remote: usize,

    ahead_main: usize,
    behind_main: usize,

    ops: Vec<&'static str>,
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
                default_branch: String::new(),
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
            let mut diagnosis = DiagnoseBranch {
                name: branch.name.clone(),
                uncommitted: 0,
                detached: false,
                gone: false,
                ahead_remote: 0,
                behind_remote: 0,
                ahead_main: 0,
                behind_main: 0,
                ops: vec![],
            };

            if branch.current {
                let lines = git.lines(&["status", "-s"])?;
                if !lines.is_empty() {
                    diagnosis.uncommitted = lines.len();
                    diagnosis.ops.push("commit");
                }
            }

            let mut need_check_rebase = true;
            if branch.name == default_branch {
                need_check_rebase = false;
            }

            match branch.status {
                BranchStatus::Detached => {
                    diagnosis.detached = true;
                    diagnosis.ops.push("create");
                    need_check_rebase = false;
                }
                BranchStatus::Gone => {
                    diagnosis.gone = true;
                    diagnosis.ops.push("delete");
                    need_check_rebase = false;
                }
                BranchStatus::Behind | BranchStatus::Ahead | BranchStatus::Conflict => {
                    let (ahead, behind) =
                        compare_logs(&git, branch.name.as_str(), default_branch.as_str())?;

                    if ahead > 0 {
                        diagnosis.ahead_remote = ahead;
                        diagnosis.ops.push("push");
                    }

                    if behind > 0 {
                        diagnosis.behind_remote = behind;
                        diagnosis.ops.push("pull");
                    }
                }
                BranchStatus::Sync => {}
            }

            if need_check_rebase {
                let (ahead, behind) =
                    compare_logs(&git, branch.name.as_str(), default_branch.as_str())?;
                if ahead > 0 {
                    diagnosis.ahead_main = ahead;
                    diagnosis.ops.push("merge");
                }
                if behind > 0 {
                    diagnosis.behind_main = behind;
                    diagnosis.ops.push("rebase");
                }
            }

            if !diagnosis.ops.is_empty() {
                branches.push(diagnosis);
            }
        }

        if branches.is_empty() {
            return Ok(None);
        }

        Ok(Some(DiagnoseResult {
            name: self.name.clone(),
            missing: false,
            default_branch,
            branches,
        }))
    }
}

impl DiagnoseResult {
    fn show(self) {
        let DiagnoseResult {
            name,
            missing,
            default_branch,
            branches,
        } = self;
        if missing {
            eprintln!("{name}: {}", style("Missing").bold().red());
            return;
        }

        let name = style(&name).bold().cyan();

        let mut has_uncommitted = false;
        let mut has_remote = false;
        let mut has_main = false;
        for branch in branches.iter() {
            if branch.uncommitted > 0 {
                has_uncommitted = true;
            }
            if branch.detached || branch.gone || branch.ahead_remote > 0 || branch.behind_remote > 0
            {
                has_remote = true;
            }
            if branch.ahead_main > 0 || branch.behind_main > 0 {
                has_main = true;
            }
        }

        let mut titles = vec![String::from("Branch")];
        if has_uncommitted {
            titles.push(String::from("Uncommitted"));
        }
        if has_remote {
            titles.push(String::from("Remote"));
        }
        if has_main {
            titles.push(default_branch);
        }
        titles.push(String::from("Operation"));

        eprintln!("{name}");
        let mut table = Table::with_capacity(branches.len());
        table.add(titles);

        for branch in branches {
            let mut row = vec![TableCell::no_color(branch.name)];
            if has_uncommitted {
                let uncommitted = if branch.uncommitted == 0 {
                    TableCell::no_color(String::new())
                } else {
                    TableCell::with_color(format!("{}", branch.uncommitted), TableCellColor::Red)
                };
                row.push(uncommitted);
            }

            if has_remote {
                let remote = if branch.detached {
                    TableCell::with_color(String::from("Detached"), TableCellColor::Red)
                } else if branch.gone {
                    TableCell::with_color(String::from("Gone"), TableCellColor::Red)
                } else if branch.ahead_remote > 0 && branch.behind_remote == 0 {
                    TableCell::with_color(
                        format!("↑{}", branch.ahead_remote),
                        TableCellColor::Green,
                    )
                } else if branch.behind_remote > 0 && branch.ahead_remote == 0 {
                    TableCell::with_color(format!("↓{}", branch.behind_remote), TableCellColor::Red)
                } else if branch.ahead_remote > 0 && branch.behind_remote > 0 {
                    TableCell::with_color(
                        format!("↑{} ↓{}", branch.ahead_remote, branch.behind_remote),
                        TableCellColor::Yellow,
                    )
                } else {
                    TableCell::no_color(String::new())
                };
                row.push(remote);
            }

            if has_main {
                let main = if branch.ahead_main > 0 && branch.behind_main == 0 {
                    TableCell::with_color(format!("↑{}", branch.ahead_main), TableCellColor::Green)
                } else if branch.behind_main > 0 && branch.ahead_main == 0 {
                    TableCell::with_color(format!("↓{}", branch.behind_main), TableCellColor::Red)
                } else if branch.ahead_main > 0 && branch.behind_main > 0 {
                    TableCell::with_color(
                        format!("↑{} ↓{}", branch.ahead_main, branch.behind_main),
                        TableCellColor::Yellow,
                    )
                } else {
                    TableCell::no_color(String::new())
                };
                row.push(main);
            }

            let ops = TableCell::no_color(branch.ops.join(","));
            row.push(ops);
            table.add_color(row);
        }

        table.show();
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

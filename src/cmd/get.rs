use std::borrow::Cow;

use anyhow::Result;
use clap::Args;
use serde::Serialize;

use crate::cmd::{Completion, Run};
use crate::config::Config;
use crate::repo::database::{Database, SelectOptions, Selector};
use crate::repo::detect::labels::DetectLabels;
use crate::repo::{NameLevel, Repo};
use crate::table::Table;
use crate::{term, utils};

/// Show repository info.
#[derive(Args)]
pub struct GetArgs {
    /// Repository selection head.
    pub head: Option<String>,

    /// Repository selection query.
    pub query: Option<String>,

    /// Show size in list info. If your workspace is large, this can cause
    /// command to take too long to execute.
    #[clap(short, long)]
    pub size: bool,

    /// Show current repo info.
    #[clap(short, long)]
    pub current: bool,

    /// Show result as json format.
    #[clap(short = 'J')]
    pub json: bool,

    /// Use the labels to filter repo.
    #[clap(short, long)]
    pub labels: Option<String>,
}

#[derive(Debug, Serialize)]
struct RepoInfo<'a> {
    remote: Cow<'a, str>,
    owner: Cow<'a, str>,
    name: Cow<'a, str>,

    accessed: u64,
    last_accessed: u64,
    last_accessed_str: String,
    score: u64,

    path: String,
    workspace: bool,

    size: u64,
    size_str: String,

    labels: Option<Vec<String>>,
}

impl RepoInfo<'_> {
    fn from_repo<'a>(
        cfg: &Config,
        repo: Repo<'a>,
        detect_labels: &Option<DetectLabels>,
    ) -> Result<RepoInfo<'a>> {
        let workspace = repo.path.is_none();
        let path = repo.get_path(cfg);
        let path = format!("{}", path.display());
        let size = utils::dir_size(repo.get_path(cfg))?;
        let labels = match detect_labels {
            Some(detect_labels) => detect_labels.sort(&repo),
            None => {
                let mut labels: Option<Vec<String>> = repo
                    .labels
                    .as_ref()
                    .map(|labels| labels.iter().map(|label| label.to_string()).collect());
                if let Some(labels) = labels.as_mut() {
                    labels.sort_unstable();
                }
                labels
            }
        };
        let score = repo.score(cfg);
        Ok(RepoInfo {
            remote: repo.remote,
            owner: repo.owner,
            name: repo.name,
            accessed: repo.accessed,
            last_accessed: repo.last_accessed,
            last_accessed_str: utils::format_time(repo.last_accessed)?,
            score,
            path,
            workspace,
            size,
            size_str: utils::human_bytes(size),
            labels,
        })
    }
}

impl Run for GetArgs {
    fn run(&self, cfg: &Config) -> Result<()> {
        let db = Database::load(cfg)?;
        let (mut repos, level) = if self.current {
            let repo = db.must_get_current()?;
            (vec![repo], NameLevel::Remote)
        } else {
            let filter_labels = utils::parse_labels(&self.labels);

            let opts = SelectOptions::default().with_filter_labels(filter_labels);
            let selector = Selector::from_args(&self.head, &self.query, opts);

            selector.many_local(&db)?
        };

        let detect_labels = if cfg.detect.auto {
            Some(DetectLabels::new(cfg))
        } else {
            None
        };

        if repos.is_empty() {
            if self.json {
                println!("{{}}");
            } else {
                println!("No repo in database");
            }
            return Ok(());
        }

        if self.json {
            let mut infos = Vec::with_capacity(repos.len());
            for repo in repos {
                infos.push(RepoInfo::from_repo(cfg, repo, &detect_labels)?);
            }
            return term::show_json(infos);
        }

        let mut table = Table::with_capacity(1 + repos.len());
        let mut titles = vec![
            String::from("Name"),
            String::from("Labels"),
            String::from("Access"),
            String::from("Time"),
            String::from("Score"),
        ];

        let mut size_vec: Option<Vec<u64>> = None;
        if self.size {
            titles.push(String::from("Size"));
            let mut repos_with_size = Vec::with_capacity(repos.len());
            for repo in repos {
                let path = repo.get_path(cfg);
                let size = utils::dir_size(path)?;
                repos_with_size.push((size, repo));
            }
            repos_with_size.sort_unstable_by(|(size1, _), (size2, _)| size2.cmp(size1));
            size_vec = Some(repos_with_size.iter().map(|(size, _)| *size).collect());
            repos = repos_with_size.into_iter().map(|(_, repo)| repo).collect();
        }
        table.add(titles);

        let mut total_access: u64 = 0;
        let mut total_score: u64 = 0;
        for (idx, repo) in repos.iter().enumerate() {
            let name = repo.to_string(&level);
            let labels = match detect_labels.as_ref() {
                Some(detect_labels) => detect_labels.format(repo),
                None => repo.labels_string(),
            }
            .unwrap_or_else(|| String::from("<none>"));
            let access = format!("{}", repo.accessed);
            total_access += repo.accessed;
            let last_access = utils::format_since(cfg, repo.last_accessed);
            let score = repo.score(cfg);
            total_score += score;
            let score = format!("{score}");

            let mut row = vec![name, labels, access, last_access, score];
            if let Some(size_vec) = size_vec.as_ref() {
                let size = utils::human_bytes(size_vec[idx]);
                row.push(size);
            }

            table.add(row);
        }

        table.foot();
        let mut foot = vec![
            format!("SUM: {}", repos.len()),
            String::from(""),
            format!("{total_access}"),
            String::from(""),
            format!("{total_score}"),
        ];
        if self.size {
            let total_size: u64 = size_vec.as_ref().unwrap().iter().sum();
            let total_size = utils::human_bytes(total_size);
            foot.push(total_size);
        }
        table.add(foot);

        table.show();
        Ok(())
    }
}

impl GetArgs {
    pub fn completion() -> Completion {
        Completion {
            args: Completion::repo_args,
            flags: Some(Completion::labels),
        }
    }
}

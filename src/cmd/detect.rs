use anyhow::{Context, Result};
use clap::Args;

use crate::cmd::{Completion, Run};
use crate::config::Config;
use crate::repo::database::{Database, SelectOptions, Selector};
use crate::repo::detect::labels::DetectLabels;
use crate::{info, term, utils};

/// Detect and update labels for repositories
#[derive(Args)]
pub struct DetectArgs {
    /// Repository selection head.
    pub head: Option<String>,

    /// Repository selection query.
    pub query: Option<String>,

    /// Remove all detect labels.
    #[clap(short = 'C', long)]
    pub clear: bool,

    /// Use the labels to filter repo.
    #[clap(short, long)]
    pub labels: Option<String>,
}

impl Run for DetectArgs {
    fn run(&self, cfg: &Config) -> Result<()> {
        let mut db = Database::load(cfg)?;

        let filter_labels = utils::parse_labels(&self.labels);

        let opts = SelectOptions::default().with_filter_labels(filter_labels);
        let selector = Selector::from_args(&self.head, &self.query, opts);

        let (repos, level) = selector.many_local(&db)?;

        let items: Vec<_> = repos.iter().map(|repo| repo.to_string(&level)).collect();
        term::must_confirm_items(&items, "detect", "detection", "Repo", "Repos")?;

        let detect_labels = DetectLabels::new(cfg);

        let repos: Vec<_> = repos.into_iter().map(|repo| repo.update()).collect();
        for mut repo in repos {
            if self.clear {
                detect_labels.clear(&mut repo);
                db.upsert(repo);
                continue;
            }

            detect_labels
                .update(&mut repo)
                .with_context(|| format!("detect labels for repo {}", repo.to_string(&level)))?;
            let labels = detect_labels
                .format(&repo)
                .unwrap_or(String::from("<none>"));
            info!("Detect for repo {}: {labels}", repo.to_string(&level));
            db.upsert(repo);
        }

        db.save()
    }
}

impl DetectArgs {
    pub fn completion() -> Completion {
        Completion {
            args: Completion::repo_args,
            flags: Some(Completion::labels),
        }
    }
}

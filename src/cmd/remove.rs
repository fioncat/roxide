use std::rc::Rc;

use anyhow::Result;
use clap::Args;

use crate::cmd::{Completion, Run};
use crate::config::Config;
use crate::repo::database::{Database, SelectOptions, Selector};
use crate::repo::Repo;
use crate::{confirm, stderrln, term, utils};

/// Remove repo(s) from database and disk.
#[derive(Args)]
pub struct RemoveArgs {
    pub head: Option<String>,

    pub query: Option<String>,

    /// Remove multiple repos.
    #[clap(short)]
    pub recursive: bool,

    /// Remove repos whose last access interval is greater than or equal to
    /// this value.
    #[clap(short)]
    pub duration: Option<String>,

    /// Remove repos whose access times are less than this value.
    #[clap(short)]
    pub access: Option<u64>,

    /// Use editor to filter items.
    #[clap(short)]
    pub edit: bool,

    /// Force remove, ignore "pin" label.
    #[clap(short)]
    pub force: bool,

    /// Use the labels to filter repo.
    #[clap(short)]
    pub labels: Option<String>,
}

impl Run for RemoveArgs {
    fn run(&self, cfg: &Config) -> Result<()> {
        let mut db = Database::load(cfg)?;
        if self.recursive {
            self.remove_many(cfg, &mut db)?;
        } else {
            self.remove_one(cfg, &mut db)?;
        }

        db.save()
    }
}

impl RemoveArgs {
    fn remove_one(&self, cfg: &Config, db: &mut Database) -> Result<()> {
        let opts = SelectOptions::default()
            .with_force_search(true)
            .with_force_local(true);
        let selector = Selector::from_args(&db, &self.head, &self.query, opts);
        let repo = selector.must_one()?;

        confirm!("Do you want to remove repo {}", repo.name_with_remote());

        let path = repo.get_path(cfg);
        utils::remove_dir_recursively(path)?;

        db.remove(repo);

        Ok(())
    }

    fn remove_many(&self, cfg: &Config, db: &mut Database) -> Result<()> {
        let filter_labels = utils::parse_labels(&self.labels);
        let opts = SelectOptions::default()
            .with_filter_labels(filter_labels)
            .with_many_edit(self.edit);
        let selector = Selector::from_args(&db, &self.head, &self.query, opts);

        let (repos, level) = selector.many_local()?;
        let repos = self.filter_many(cfg, repos)?;
        if repos.is_empty() {
            stderrln!("No repo to remove");
            return Ok(());
        }

        let items: Vec<_> = repos.iter().map(|repo| repo.to_string(&level)).collect();
        term::must_confirm_items(&items, "remove", "removal", "Repo", "Repos")?;

        for repo in repos.into_iter() {
            let path = repo.get_path(cfg);
            utils::remove_dir_recursively(path)?;
            db.remove(repo);
        }

        Ok(())
    }

    fn filter_many(&self, cfg: &Config, repos: Vec<Rc<Repo>>) -> Result<Vec<Rc<Repo>>> {
        let duration = match self.duration.as_ref() {
            Some(s) => Some(utils::parse_duration_secs(s)?),
            None => None,
        };

        let pin_label = String::from("pin");

        let mut result = Vec::with_capacity(repos.len());
        for repo in repos {
            if let Some(d) = duration {
                let delta = cfg.now().saturating_sub(repo.last_accessed);
                if delta < d {
                    continue;
                }
            }

            if let Some(access) = self.access {
                let access = access as f64;
                if repo.accessed > access {
                    continue;
                }
            }

            if !self.force {
                if let Some(labels) = repo.labels.as_ref() {
                    if labels.contains(&pin_label) {
                        continue;
                    }
                }
            }

            result.push(repo);
        }
        Ok(result)
    }

    pub fn completion() -> Completion {
        Completion {
            args: Completion::repo_args,
            flags: Some(Completion::labels),
        }
    }
}

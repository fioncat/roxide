use std::borrow::Cow;

use anyhow::{bail, Result};
use clap::Args;

use crate::cmd::{Completion, Run};
use crate::config::Config;
use crate::repo::database::{Database, SelectOptions, Selector};
use crate::utils;

/// Add or remove labels for repository
#[derive(Args)]
pub struct LabelArgs {
    /// Repository selection head.
    pub head: Option<String>,

    /// Repository selection query.
    pub query: Option<String>,

    /// Use search instead of fuzzy matching.
    #[clap(short)]
    pub search: bool,

    /// When calling the remote API, ignore caches that are not expired.
    #[clap(short)]
    pub force: bool,

    /// Set labels with given values.
    #[clap(short = 'l')]
    pub set: Option<String>,

    /// Append labels with given values.
    #[clap(short)]
    pub append: Option<String>,

    /// Delete given labels.
    #[clap(short)]
    pub delete: Option<String>,

    /// Clean all labels.
    #[clap(short)]
    pub clean: bool,
}

impl Run for LabelArgs {
    fn run(&self, cfg: &Config) -> Result<()> {
        let mut db = Database::load(cfg)?;

        let opts = SelectOptions::default()
            .with_force_search(self.search)
            .with_force_no_cache(self.force);
        let selector = Selector::from_args(&self.head, &self.query, opts);

        let mut repo = selector.must_one(&db)?;

        if let Some(set_labels) = self.set.as_ref() {
            let set_labels = utils::parse_labels_str(set_labels);
            repo.labels = Some(
                set_labels
                    .into_iter()
                    .map(|label| Cow::Owned(label))
                    .collect(),
            );
        } else if let Some(append_labels) = self.append.as_ref() {
            let append_labels = utils::parse_labels_str(append_labels);
            repo.append_labels(Some(append_labels));
        } else if let Some(delete_labels) = self.delete.as_ref() {
            let delete_labels = utils::parse_labels_str(delete_labels);
            for to_delete in delete_labels {
                if let Some(labels) = repo.labels.as_mut() {
                    labels.remove(to_delete.as_str());
                }
            }
        } else if self.clean {
            repo.labels = None;
        } else {
            bail!("please specify at least one operation");
        }

        db.upsert(repo.update());

        db.save()
    }
}

impl LabelArgs {
    pub fn completion() -> Completion {
        Completion {
            args: Completion::repo_args,
            flags: Some(|cfg, flag, to_complete| match flag {
                'l' | 'a' | 'd' => Completion::labels_flag(cfg, to_complete),
                _ => Ok(None),
            }),
        }
    }
}

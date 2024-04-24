use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use anyhow::{bail, Result};
use clap::Args;

use crate::batch::{self, Task};
use crate::cmd::Run;
use crate::config::Config;
use crate::repo::database::{Database, SelectOptions, Selector};
use crate::repo::detect::stats::{DetectStats, LanguageStats};
use crate::term::Table;
use crate::utils;

/// Count and display repository code stats.
#[derive(Args)]
pub struct StatsArgs {
    /// Repository selection head.
    pub head: Option<String>,

    /// Repository selection query.
    pub query: Option<String>,

    /// Stats multiple.
    #[clap(short, long)]
    pub recursive: bool,

    /// Use the labels to filter repository.
    #[clap(short, long)]
    pub labels: Option<String>,
}

impl Run for StatsArgs {
    fn run(&self, cfg: &Config) -> Result<()> {
        let start = Instant::now();
        let db = Database::load(cfg)?;
        let mut stats = if self.recursive {
            self.stats_many(cfg, &db)
        } else {
            self.stats_one(cfg, &db)
        }?;

        if stats.is_empty() {
            eprintln!("no stats to show");
            return Ok(());
        }

        stats.sort_unstable_by(|a, b| b.code.cmp(&a.code));
        let mut table = Table::with_capacity(stats.len());
        table.add(vec![
            String::from("Language"),
            String::from("files"),
            String::from("blank"),
            String::from("comment"),
            String::from("code"),
        ]);

        let mut files: usize = 0;
        let mut blank: usize = 0;
        let mut comment: usize = 0;
        let mut code: usize = 0;
        let name_tail = " ".repeat(8);
        for lang in stats.iter() {
            let mut name = String::from(lang.name);
            name.push_str(&name_tail);
            table.add(vec![
                name,
                format!("{}", lang.files),
                format!("{}", lang.blank),
                format!("{}", lang.comment),
                format!("{}", lang.code),
            ]);

            files += lang.files;
            blank += lang.blank;
            comment += lang.comment;
            code += lang.code;
        }

        if stats.len() > 1 {
            table.foot();
            table.add(vec![
                String::from("SUM"),
                format!("{files}"),
                format!("{blank}"),
                format!("{comment}"),
                format!("{code}"),
            ]);
        }

        self.show_speed(start, files, blank + comment + code);
        table.show();

        Ok(())
    }
}

impl StatsArgs {
    fn stats_one(&self, cfg: &Config, db: &Database) -> Result<Vec<LanguageStats>> {
        let repo = if self.head.is_none() {
            db.must_get_current()?
        } else {
            let opts = SelectOptions::default()
                .with_force_search(true)
                .with_force_local(true);
            let selector = Selector::from_args(&self.head, &self.query, opts);
            selector.must_one(db)?
        };

        let detect_stats = DetectStats::new(cfg);
        let path = repo.get_path(cfg);
        detect_stats.count(&path)
    }

    fn stats_many(&self, cfg: &Config, db: &Database) -> Result<Vec<LanguageStats>> {
        let filter_labels = utils::parse_labels(&self.labels);
        let opts = SelectOptions::default().with_filter_labels(filter_labels);
        let selector = Selector::from_args(&self.head, &self.query, opts);

        let (repos, level) = selector.many_local(db)?;
        if repos.is_empty() {
            bail!("no repo to count stats");
        }

        let detect_stats = Arc::new(DetectStats::new(cfg));

        let mut tasks = Vec::with_capacity(repos.len());
        for repo in repos {
            let name = repo.to_string(&level);
            let task = StatsTask {
                detect_stats: Arc::clone(&detect_stats),
                path: repo.get_path(cfg),
            };
            tasks.push((name, task));
        }

        let all_stats = batch::must_run("Stats", tasks)?;
        eprintln!();

        let mut result: HashMap<&str, LanguageStats> = HashMap::new();
        for stats in all_stats {
            for lang in stats {
                match result.get_mut(lang.name) {
                    Some(result_lang) => {
                        result_lang.files += lang.files;
                        result_lang.blank += lang.blank;
                        result_lang.comment += lang.comment;
                        result_lang.code += lang.code;
                    }
                    None => {
                        result.insert(lang.name, lang);
                    }
                }
            }
        }

        let result: Vec<_> = result.into_values().collect();
        Ok(result)
    }

    fn show_speed(&self, start: Instant, files: usize, lines: usize) {
        let file_word = if files > 1 { "files" } else { "file" };
        let line_word = if lines > 1 { "lines" } else { "line" };
        eprint!("Stats: {files} {file_word}; {lines} {line_word}");

        let elapsed_seconds = start.elapsed().as_secs_f64();
        if elapsed_seconds > 0.0 {
            let speed = lines as f64 / elapsed_seconds;
            let speed = speed as u64;
            if speed > 1 {
                eprint!("; {speed} lines/s");
            }
        }

        eprintln!();
    }
}

struct StatsTask {
    detect_stats: Arc<DetectStats>,

    path: PathBuf,
}

impl Task<Vec<LanguageStats>> for StatsTask {
    fn run(&self) -> Result<Vec<LanguageStats>> {
        self.detect_stats.count(&self.path)
    }
}

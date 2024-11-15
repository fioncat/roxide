use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use anyhow::{bail, Context, Result};
use clap::Args;
use console::style;

use crate::batch::{self, Task};
use crate::cmd::{Completion, CompletionResult, Run};
use crate::config::Config;
use crate::repo::database::{Database, SelectOptions, Selector};
use crate::repo::detect::stats::{DetectStats, LanguageStats, LanguageStatsChange, StatsStorage};
use crate::table::{Table, TableCell, TableCellColor};
use crate::{confirm, utils};

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

    /// Remove stats storage.
    #[clap(short, long)]
    pub delete: Option<Option<String>>,

    /// Show saved stats.
    #[clap(short, long)]
    pub name: Option<Option<String>>,

    /// Compare with given saved stats.
    #[clap(short, long)]
    pub compare: Option<Option<String>>,

    /// Save current stats.
    #[clap(short, long)]
    pub save: bool,
}

impl Run for StatsArgs {
    fn run(&self, cfg: &Config) -> Result<()> {
        let storage = StatsStorage::load(cfg)?;

        if let Some(name) = self.delete.as_ref() {
            if let Some(name) = name {
                confirm!("Do you want to remove stats {}", name);
            } else {
                confirm!("Do you want to remove all saved stats");
            }

            storage.remove(name)?;

            return Ok(());
        }

        let (mut stats, start) = if let Some(name) = self.name.as_ref() {
            if self.save {
                bail!("when using `-n` to show stats, cannot use `-s` to save it again");
            }
            let (stats, name) = storage.get(name)?;
            eprintln!("Show saved stats: {name}");
            (stats, None)
        } else {
            let start = Instant::now();
            let db = Database::load(cfg)?;
            let stats = if self.recursive {
                self.stats_many(cfg, &db)
            } else {
                self.stats_one(cfg, &db)
            }?;
            (stats, Some(start))
        };

        if stats.is_empty() {
            eprintln!("no stats to show");
            return Ok(());
        }

        let mut total_files: usize = 0;
        let mut total_lines: usize = 0;
        let mut total_blank: usize = 0;
        let mut total_comment: usize = 0;
        let mut total_code: usize = 0;
        for lang in stats.iter_mut() {
            total_files += lang.files;
            lang.lines = lang.blank + lang.comment + lang.code;
            total_lines += lang.lines;

            total_blank += lang.blank;
            total_comment += lang.comment;
            total_code += lang.code;
        }

        let total_lines_f64 = total_lines as f64;
        if total_lines_f64 <= 0.0 {
            bail!("no line count");
        }

        for lang in stats.iter_mut() {
            assert!(total_lines_f64 > 0.0);
            lang.percent = (lang.lines as f64 / total_lines_f64) * 100.0;
        }
        stats.sort_unstable_by(|a, b| b.lines.cmp(&a.lines));

        let save_stats = if self.save { Some(stats.clone()) } else { None };
        let compare_stats = if self.compare.is_some() {
            Some(stats.clone())
        } else {
            None
        };

        let mut table = Table::with_capacity(stats.len());
        table.add(vec![
            String::from("Language"),
            String::from("files"),
            String::from("blank"),
            String::from("comment"),
            String::from("code"),
            String::from("lines"),
            String::from("percent"),
        ]);

        let name_tail = " ".repeat(8);
        let need_foot = stats.len() > 1;
        for lang in stats {
            let LanguageStats {
                name,
                files,
                blank,
                comment,
                code,
                lines,
                percent,
            } = lang;

            let mut name = name.into_owned();
            name.push_str(&name_tail);

            table.add(vec![
                name,
                format!("{files}"),
                format!("{blank}"),
                format!("{comment}"),
                format!("{code}"),
                format!("{lines}"),
                format!("{:.2}%", percent),
            ]);
        }

        if need_foot {
            table.foot();
            table.add(vec![
                String::from("SUM"),
                format!("{total_files}"),
                format!("{total_blank}"),
                format!("{total_comment}"),
                format!("{total_code}"),
                format!("{total_lines}"),
                String::new(),
            ]);
        }

        if let Some(start) = start {
            self.show_speed(start, total_files, total_lines);
        }
        table.show();

        if let Some(stats) = compare_stats {
            let (target, name) = storage
                .get(self.compare.as_ref().unwrap())
                .context("get compare target")?;

            eprintln!();
            eprintln!("Compare with: {}", style(name).magenta().bold());

            self.show_compare(target, stats);
        }

        if let Some(stats) = save_stats {
            let name = storage.save(stats)?;
            eprintln!();
            eprintln!("Save stats: {name}");
        }

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

        let mut result: HashMap<String, LanguageStats> = HashMap::new();
        for stats in all_stats {
            for lang in stats {
                match result.get_mut(lang.name.as_ref()) {
                    Some(result_lang) => {
                        result_lang.files += lang.files;
                        result_lang.blank += lang.blank;
                        result_lang.comment += lang.comment;
                        result_lang.code += lang.code;
                    }
                    None => {
                        result.insert(lang.name.to_string(), lang);
                    }
                }
            }
        }

        let result: Vec<_> = result.into_values().collect();
        Ok(result)
    }

    fn show_compare(&self, old: Vec<LanguageStats>, current: Vec<LanguageStats>) {
        let changes = LanguageStatsChange::compare(old, current);
        if changes.is_empty() {
            eprintln!("<Nothing changed>");
            return;
        }
        let mut table = Table::with_capacity(changes.len());
        table.add(vec![
            String::from("Language"),
            String::from("files"),
            String::from("blank"),
            String::from("comment"),
            String::from("code"),
            String::from("lines"),
            String::from("percent"),
        ]);

        let change_cell = |num: i64| -> TableCell {
            if num == 0 {
                return TableCell::no_color(String::from("0"));
            }
            if num > 0 {
                TableCell::with_color(format!("+{num}"), TableCellColor::Green)
            } else {
                TableCell::with_color(format!("{num}"), TableCellColor::Red)
            }
        };

        let need_foot = changes.len() > 1;
        let name_tail = " ".repeat(8);

        let mut total_files: i64 = 0;
        let mut total_lines: i64 = 0;
        let mut total_blank: i64 = 0;
        let mut total_comment: i64 = 0;
        let mut total_code: i64 = 0;
        for change in changes {
            let LanguageStatsChange {
                name,
                files,
                blank,
                comment,
                code,
                lines,
                lines_abs: _,
                percent,
            } = change;

            let mut name = name.into_owned();
            name.push_str(&name_tail);

            total_files += files;
            total_lines += lines;
            total_blank += blank;
            total_comment += comment;
            total_code += code;

            table.add_color(vec![
                TableCell::no_color(name),
                change_cell(files),
                change_cell(blank),
                change_cell(comment),
                change_cell(code),
                change_cell(lines),
                TableCell::no_color(format!("{:.2}%", percent)),
            ]);
        }

        if need_foot {
            table.foot();
            table.add_color(vec![
                TableCell::no_color(String::from("SUM")),
                change_cell(total_files),
                change_cell(total_blank),
                change_cell(total_comment),
                change_cell(total_code),
                change_cell(total_lines),
                TableCell::no_color(String::new()),
            ]);
        }

        table.show();
    }

    fn show_speed(&self, start: Instant, files: usize, lines: usize) {
        let elapsed_seconds = start.elapsed().as_secs_f64();
        eprint!("Speed: ");

        if elapsed_seconds > 0.0 {
            let files_speed = (files as f64 / elapsed_seconds) as u64;
            let lines_speed = (lines as f64 / elapsed_seconds) as u64;

            if files_speed > 1 && lines_speed > 1 {
                eprint!("{files_speed} files/s; {lines_speed} lines/s");
            } else {
                eprint!("WTF??? Too low to be shown, what machine are you using???");
            }
        } else {
            eprint!("Wow! Too fast to be shown!");
        }

        eprintln!();
    }

    pub fn completion() -> Completion {
        Completion {
            args: Completion::repo_args,
            flags: Some(|cfg, flag, to_complete| match flag {
                'l' => Completion::labels_flag(cfg, to_complete),
                'n' | 'd' | 'c' => {
                    let storage = StatsStorage::load(cfg)?;
                    if !to_complete.contains('_') {
                        let dates = storage.list_dates()?;
                        return Ok(Some(CompletionResult::from(dates)));
                    }
                    let mut fields = to_complete.split('_');
                    let date = fields.next();
                    let date = match date {
                        Some(date) => date,
                        None => return Ok(None),
                    };
                    let count = storage.date_count(date)?;
                    let mut items = Vec::with_capacity(count);
                    for i in 0..count {
                        let item = format!("{date}_{i}");
                        items.push(item);
                    }
                    Ok(Some(CompletionResult::from(items)))
                }
                _ => Ok(None),
            }),
        }
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

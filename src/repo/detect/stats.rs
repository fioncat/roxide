use std::borrow::Cow;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use chrono::{Local, NaiveDate};
use glob::Pattern as GlobPattern;
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::utils::FileLock;
use crate::{utils, warn};

use super::{Language, LanguageGroup};

pub struct DetectStats {
    languages: Vec<Language>,

    ignores: Vec<GlobPattern>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LanguageStats {
    pub name: Cow<'static, str>,

    pub files: usize,

    pub blank: usize,
    pub comment: usize,
    pub code: usize,

    pub lines: usize,
    pub percent: f64,
}

impl DetectStats {
    pub fn new(cfg: &Config) -> Self {
        Self {
            languages: super::builtin_languages(),
            ignores: cfg.detect_ignores.clone(),
        }
    }

    pub fn count(&self, path: &Path) -> Result<Vec<LanguageStats>> {
        let groups = super::detect_languages(&self.ignores, path, &self.languages)?;
        let mut result = Vec::with_capacity(groups.len());

        for group in groups {
            let LanguageGroup { language, files } = group;
            let mut stats = LanguageStats {
                name: Cow::Borrowed(language.name),
                files: files.len(),
                blank: 0,
                comment: 0,
                code: 0,
                lines: 0,
                percent: 0.0,
            };

            for file in files {
                self.count_file(path, file, &language, &mut stats)?;
            }

            if stats.blank == 0 && stats.comment == 0 && stats.code == 0 {
                continue;
            }
            result.push(stats);
        }

        Ok(result)
    }

    fn count_file(
        &self,
        path: &Path,
        file: String,
        language: &Language,
        stats: &mut LanguageStats,
    ) -> Result<()> {
        let file_path = PathBuf::from(path).join(file);
        let file = File::open(&file_path).with_context(|| {
            format!(
                "open file '{}' for counting code stats",
                file_path.display()
            )
        })?;
        let reader = BufReader::new(file);

        for line in reader.lines() {
            let line = line.with_context(|| {
                format!(
                    "read line from file '{}' for counting code stats",
                    file_path.display()
                )
            })?;

            let line = line.trim();
            if line.is_empty() {
                stats.blank += 1;
                continue;
            }

            let mut is_comment = false;
            for comment in language.comments.iter() {
                if line.starts_with(*comment) {
                    is_comment = true;
                    break;
                }
            }

            if is_comment {
                stats.comment += 1;
            } else {
                stats.code += 1;
            }
        }

        Ok(())
    }
}

pub struct StatsStorage {
    dir: PathBuf,

    _lock: FileLock,
}

impl StatsStorage {
    const DATE_FORMAT: &'static str = "%Y-%m-%d";

    pub fn load(cfg: &Config) -> Result<Self> {
        let lock = FileLock::acquire(cfg, "stats")?;

        let path = cfg.get_meta_dir().join("stats");
        utils::ensure_dir(&path.join(".keep"))?;

        Ok(Self {
            dir: path,
            _lock: lock,
        })
    }

    pub fn save(&self, stats: Vec<LanguageStats>) -> Result<String> {
        let date = Local::now().date_naive();
        let date = date.format(Self::DATE_FORMAT).to_string();
        let path = self.dir.join(&date);

        let mut data = self.read_data(&path)?;
        data.push(stats);

        self.write_data(&path, &data)?;

        Ok(format!("{date}_{}", data.len() - 1))
    }

    pub fn list_dates(&self) -> Result<Vec<String>> {
        let read_dir = fs::read_dir(&self.dir)
            .with_context(|| format!("read stats dir '{}'", self.dir.display()))?;
        let mut dates = Vec::new();
        for entry in read_dir {
            let entry =
                entry.with_context(|| format!("read entry from dir '{}'", self.dir.display()))?;
            let name = entry.file_name();
            let name = match name.to_str() {
                Some(name) => name,
                None => continue,
            };

            let date = match NaiveDate::parse_from_str(name, Self::DATE_FORMAT) {
                Ok(date) => date,
                Err(_) => {
                    warn!(
                        "Invalid stats item '{}' in dir '{}', please consider delete it manually",
                        name,
                        self.dir.display()
                    );
                    continue;
                }
            };

            dates.push((String::from(name), date));
        }

        dates.sort_unstable_by(|(_, date0), (_, date1)| date1.cmp(date0));
        let dates = dates.into_iter().map(|(name, _)| name).collect();
        Ok(dates)
    }

    pub fn date_count(&self, name: &str) -> Result<usize> {
        let path = self.dir.join(name);
        let data = self.read_data(&path)?;
        Ok(data.len())
    }

    pub fn get(&self, name: &Option<String>) -> Result<(Vec<LanguageStats>, String)> {
        let (name, index) = match name.as_ref() {
            Some(name) => self.parse_name(name)?,
            None => {
                let mut dates = self.list_dates()?;
                if dates.is_empty() {
                    bail!("no stats saved");
                }
                (Cow::Owned(dates.remove(0)), -1)
            }
        };

        let path = self.dir.join(name.as_ref());
        let mut data = self.read_data(&path)?;
        if data.is_empty() {
            bail!("cannot find the stats");
        }

        let index: usize = if index < 0 {
            data.len() - 1
        } else {
            index as usize
        };

        if index >= data.len() {
            bail!("index {index} is out of bound of stats");
        }

        Ok((data.remove(index), format!("{name}_{index}")))
    }

    pub fn remove(&self, name: &Option<String>) -> Result<()> {
        if name.is_none() {
            let dates = self.list_dates()?;
            for date in dates {
                let path = self.dir.join(&date);
                fs::remove_file(&path)
                    .with_context(|| format!("remove listed stats file '{}'", path.display()))?;
            }
            return Ok(());
        }

        let name = name.as_ref().unwrap();

        let (name, index) = self.parse_name(name)?;
        let path = self.dir.join(name.as_ref());

        let remove_stats = || -> Result<()> {
            fs::remove_file(&path)
                .with_context(|| format!("remove stats file '{}'", path.display()))
        };

        if index < 0 {
            return remove_stats();
        }

        let mut data = self.read_data(&path)?;
        let index = index as usize;
        if index >= data.len() {
            bail!("index {index} is out of bound of stats");
        }
        data.remove(index);

        if data.is_empty() {
            return remove_stats();
        }

        self.write_data(&path, &data)
    }

    fn parse_name<'a>(&self, name: &'a str) -> Result<(Cow<'a, str>, i32)> {
        let (date, index) = if name.contains('_') {
            let fields: Vec<_> = name.split('_').collect();
            if fields.len() != 2 {
                bail!("invalid stats name format '{name}', expect 'date_index'");
            }
            let mut iter = fields.into_iter();
            let date = iter.next().unwrap();
            let index = iter.next().unwrap();
            let index: i32 = index
                .parse()
                .with_context(|| format!("invalid index in stats name '{name}', expect number"))?;
            (Cow::Owned(String::from(date)), index)
        } else {
            (Cow::Borrowed(name), -1)
        };

        if NaiveDate::parse_from_str(&date, Self::DATE_FORMAT).is_err() {
            bail!("invalid stats name '{name}', the date is invalid");
        }

        Ok((date, index))
    }

    fn read_data(&self, path: &Path) -> Result<Vec<Vec<LanguageStats>>> {
        match fs::read(path) {
            Ok(data) => {
                let data: Vec<Vec<LanguageStats>> = serde_json::from_slice(&data)
                    .with_context(|| format!("invalid json data in '{}'", path.display()))?;
                Ok(data)
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(err) => Err(err).with_context(|| format!("read stats file '{}'", path.display())),
        }
    }

    fn write_data(&self, path: &Path, data: &Vec<Vec<LanguageStats>>) -> Result<()> {
        let data = serde_json::to_vec(&data).context("encode stats json data")?;
        utils::ensure_dir(path)?;
        fs::write(path, data)
            .with_context(|| format!("write stats json to file '{}'", path.display()))
    }
}

pub struct LanguageStatsChange {
    pub name: Cow<'static, str>,

    pub files: i64,

    pub blank: i64,
    pub comment: i64,
    pub code: i64,

    pub lines: i64,

    pub lines_abs: usize,
    pub percent: f64,
}

impl LanguageStatsChange {
    pub fn compare(old: Vec<LanguageStats>, current: Vec<LanguageStats>) -> Vec<Self> {
        let mut old_map: HashMap<_, _> = old
            .into_iter()
            .map(|lang| (lang.name.clone(), lang))
            .collect();

        let mut changes = Vec::with_capacity(current.len());
        let mut lines_total: usize = 0;
        for stats in current {
            let old = match old_map.remove(stats.name.as_ref()) {
                Some(old) => old,
                None => {
                    lines_total += stats.lines;
                    // A new added language
                    changes.push(LanguageStatsChange {
                        name: stats.name,
                        files: stats.files as _,
                        blank: stats.blank as _,
                        comment: stats.comment as _,
                        code: stats.code as _,
                        lines: stats.lines as _,
                        lines_abs: stats.lines,
                        percent: 0.0,
                    });
                    continue;
                }
            };

            let change_files = stats.files as i64 - old.files as i64;

            let change_blank = stats.blank as i64 - old.blank as i64;
            let change_comment = stats.comment as i64 - old.comment as i64;
            let change_code = stats.code as i64 - old.code as i64;

            let change_lines = stats.lines as i64 - old.lines as i64;

            if change_files == 0
                && change_blank == 0
                && change_comment == 0
                && change_code == 0
                && change_lines == 0
            {
                // Nothing changes, skip this language
                continue;
            }

            let lines_abs =
                (change_blank.abs() + change_comment.abs() + change_code.abs()) as usize;
            lines_total += lines_abs;

            changes.push(LanguageStatsChange {
                name: stats.name,
                files: change_files,
                blank: change_blank,
                comment: change_comment,
                code: change_code,
                lines: change_lines,
                lines_abs,
                percent: 0.0,
            });
        }

        for stats in old_map.into_values() {
            lines_total += stats.lines;
            // This language is deleted
            changes.push(LanguageStatsChange {
                name: stats.name,
                files: -(stats.files as i64),
                blank: -(stats.blank as i64),
                comment: -(stats.comment as i64),
                code: -(stats.code as i64),
                lines: -(stats.lines as i64),
                lines_abs: stats.lines as _,
                percent: 0.0,
            });
        }

        for change in changes.iter_mut() {
            assert!(lines_total > 0);
            let percent = (change.lines_abs as f64 / lines_total as f64) * 100.0;
            change.percent = percent;
        }

        changes.sort_unstable_by(|a, b| b.lines_abs.cmp(&a.lines_abs));
        changes
    }
}

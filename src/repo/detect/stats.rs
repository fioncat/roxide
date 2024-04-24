use std::borrow::Cow;
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

    pub fn get(&self, name: &Option<String>) -> Result<Vec<LanguageStats>> {
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

        Ok(data.remove(index))
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

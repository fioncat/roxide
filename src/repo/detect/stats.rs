use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use glob::Pattern as GlobPattern;
use serde::{Deserialize, Serialize};

use crate::config::Config;

use super::{Language, LanguageGroup};

pub struct DetectStats {
    languages: Vec<Language>,

    ignores: Vec<GlobPattern>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LanguageStats {
    pub name: &'static str,

    pub files: usize,

    pub blank: usize,
    pub comment: usize,
    pub code: usize,
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
                name: language.name,
                files: files.len(),
                blank: 0,
                comment: 0,
                code: 0,
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

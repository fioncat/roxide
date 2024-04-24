use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::fs;

use anyhow::{Context, Result};

use crate::config::Config;
use crate::repo::Repo;

use super::{Language, Module};

pub struct DetectLabels<'a> {
    languages: Vec<Language>,
    language_labels: HashSet<&'static str>,

    modules: HashMap<&'static str, Module>,

    builtin_labels: HashSet<&'static str>,

    cfg: &'a Config,
}

impl<'a> DetectLabels<'a> {
    pub fn new(cfg: &'a Config) -> Self {
        let languages = super::builtin_languages();
        let modules = super::builtin_modules();

        let mut builtin_labels = HashSet::with_capacity(languages.len() + modules.len());
        let mut language_labels = HashSet::with_capacity(languages.len());
        for lang in languages.iter() {
            builtin_labels.insert(lang.label);
            language_labels.insert(lang.label);
        }
        for label in modules.keys() {
            builtin_labels.insert(*label);
        }

        Self {
            languages,
            language_labels,
            modules,
            builtin_labels,
            cfg,
        }
    }

    pub fn update(&self, repo: &mut Repo) -> Result<()> {
        let mut labels: HashSet<Cow<str>> = match repo.labels.take() {
            Some(labels) => self._clear(labels),
            None => return Ok(()),
        };

        let path = repo.get_path(self.cfg);

        let root_entries = fs::read_dir(&path)?;
        let mut root_files = HashSet::new();
        let mut root_dirs = HashSet::new();
        for entry in root_entries {
            let entry = entry.context("read entry from repo root directory")?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let info = entry
                .metadata()
                .context("read metadata from repo root directory")?;
            if info.is_dir() {
                root_dirs.insert(name);
                continue;
            }
            root_files.insert(name);
        }

        let groups = super::detect_languages(self.cfg, &path, &self.languages)?;
        for group in groups {
            labels.insert(Cow::Borrowed(group.language.label));
        }

        for (label, module) in self.modules.iter() {
            let mut found = false;
            for require_label in module.require.iter() {
                if labels.contains(*require_label) {
                    found = true;
                    break;
                }
            }
            if !found {
                continue;
            }

            found = false;
            if let Some(files) = module.files.as_ref() {
                for file in files.iter() {
                    if root_files.contains(*file) {
                        found = true;
                        break;
                    }
                }
            }
            if let Some(dirs) = module.dirs.as_ref() {
                for dir in dirs.iter() {
                    if root_dirs.contains(*dir) {
                        found = true;
                        break;
                    }
                }
            }
            if !found {
                continue;
            }

            labels.insert(Cow::Borrowed(*label));
        }

        repo.labels = Some(labels);
        Ok(())
    }

    #[inline]
    pub fn clear(&self, repo: &mut Repo) {
        if let Some(labels) = repo.labels.take() {
            let clear_labels = self._clear(labels);
            repo.labels = Some(clear_labels);
        }
    }

    #[inline]
    fn _clear<'b>(&self, labels: HashSet<Cow<'b, str>>) -> HashSet<Cow<'b, str>> {
        labels
            .into_iter()
            .filter(|label| !self.builtin_labels.contains(label.as_ref()))
            .collect()
    }

    pub fn sort(&self, repo: &Repo) -> Option<Vec<String>> {
        let raw_labels = repo.labels.as_ref()?;

        let mut user_labels = Vec::with_capacity(raw_labels.len());
        let mut ext_labels = Vec::with_capacity(raw_labels.len());
        let mut module_labels = Vec::with_capacity(raw_labels.len());
        for label in raw_labels.iter() {
            if self.language_labels.contains(label.as_ref()) {
                ext_labels.push(label.to_string());
                continue;
            }
            if self.modules.contains_key(label.as_ref()) {
                module_labels.push(label.to_string());
                continue;
            }
            user_labels.push(label.to_string());
        }

        user_labels.sort_unstable();
        ext_labels.sort_unstable();
        module_labels.sort_unstable();

        user_labels.extend(ext_labels);
        user_labels.extend(module_labels);

        Some(user_labels)
    }

    #[inline]
    pub fn format(&self, repo: &Repo) -> Option<String> {
        let labels = self.sort(repo)?;
        Some(labels.join(","))
    }
}

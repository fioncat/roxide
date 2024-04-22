use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::config::Config;
use crate::repo::Repo;
use crate::term;

macro_rules! map {
    ($($k:expr => $v:expr),* $(,)?) => {{
        core::convert::From::from([$(($k, $v),)*])
    }};
}

fn build_extensions() -> HashMap<&'static str, Vec<&'static str>> {
    map![
        "c" => vec!["c", "h"],
        "cpp" => vec!["cpp", "cc", "C", "hpp"],
        "csharp" => vec!["cs"],
        "golang" => vec!["go"],
        "java" => vec!["java"],
        "js" => vec!["js"],
        "kotlin" => vec!["kt", "kts"],
        "lua" => vec!["lua"],
        "perl" => vec!["pl"],
        "php" => vec!["php"],
        "python" => vec!["py"],
        "r" => vec!["R"],
        "ruby" => vec!["rb"],
        "rust" => vec!["rs"],
        "scala" => vec!["scala"],
        "ts" => vec!["ts"],
        "web" => vec!["html", "css"],
    ]
}

struct Module {
    require: Vec<&'static str>,
    files: Option<Vec<&'static str>>,
    dirs: Option<Vec<&'static str>>,
}

fn build_modules() -> HashMap<&'static str, Module> {
    map![
        "cargo" => Module{
            require: vec!["rust"],
            files: Some(vec!["Cargo.toml"]),
            dirs: None,
        },
        "composer" => Module {
            require: vec!["php"],
            files: Some(vec!["composer.json"]),
            dirs: None,
        },
        "cmake" => Module{
            require: vec!["c", "cpp"],
            files: Some(vec!["CMakeLists.txt"]),
            dirs: None,
        },
        "gomod" => Module {
            require: vec!["golang"],
            files: Some(vec!["go.mod", "go.work"]),
            dirs: None,
        },
        "govendor" => Module {
            require: vec!["golang"],
            files: None,
            dirs: Some(vec!["vendor"]),
        },
        "maven" => Module {
            require: vec!["java"],
            files: Some(vec!["pom.xml"]),
            dirs: None,
        },
        "gradle" => Module {
            require: vec!["java", "kotlin"],
            files: Some(vec!["settings.gradle", "build.gradle"]),
            dirs: None,
        },
        "nodejs" => Module {
            require: vec!["js", "ts", "web"],
            files: Some(vec!["package.json"]),
            dirs: None,
        },
        "gem" => Module {
            require: vec!["ruby"],
            files: Some(vec!["Gemfile"]),
            dirs: None,
        },

    ]
}

pub struct Detect<'a> {
    label_extensions: HashMap<&'static str, Vec<&'static str>>,
    modules: HashMap<&'static str, Module>,

    builtin_labels: HashSet<&'static str>,

    cfg: &'a Config,
}

impl<'a> Detect<'a> {
    pub fn new(cfg: &'a Config) -> Self {
        let label_extensions = build_extensions();
        let modules = build_modules();

        let mut builtin_labels = HashSet::with_capacity(label_extensions.len() + modules.len());
        for label in label_extensions.keys() {
            builtin_labels.insert(*label);
        }
        for label in modules.keys() {
            builtin_labels.insert(*label);
        }

        Self {
            label_extensions,
            modules,
            builtin_labels,
            cfg,
        }
    }

    pub fn update_labels(&self, cfg: &Config, repo: &mut Repo) -> Result<()> {
        let mut labels: HashSet<Cow<str>> = match repo.labels.take() {
            Some(labels) => labels
                .into_iter()
                .filter(|label| !self.builtin_labels.contains(label.as_ref()))
                .collect(),
            None => return Ok(()),
        };

        let path = repo.get_path(cfg);
        let git_extensions = self.scan_git_extensions(&path)?;

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

        for (label, extensions) in self.label_extensions.iter() {
            for extension in extensions.iter() {
                if git_extensions.contains(*extension) {
                    labels.insert(Cow::Borrowed(*label));
                    break;
                }
            }
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

    pub fn sort_labels(&self, repo: &Repo) -> Option<Vec<String>> {
        let raw_labels = repo.labels.as_ref()?;

        let mut user_labels = Vec::with_capacity(raw_labels.len());
        let mut ext_labels = Vec::with_capacity(raw_labels.len());
        let mut module_labels = Vec::with_capacity(raw_labels.len());
        for label in raw_labels.iter() {
            if self.label_extensions.contains_key(label.as_ref()) {
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
    pub fn format_labels(&self, repo: &Repo) -> Option<String> {
        let labels = self.sort_labels(repo)?;
        Some(labels.join(","))
    }

    fn scan_git_extensions(&self, path: &Path) -> Result<HashSet<String>> {
        let items = term::list_git_files(path, &self.cfg.detect_ignores)?;
        let mut extensions = HashSet::new();
        for item in items {
            let item_path = PathBuf::from(item);
            if let Some(Some(ext)) = item_path.extension().map(|ext| ext.to_str()) {
                if extensions.contains(ext) {
                    continue;
                }
                extensions.insert(String::from(ext));
            }
        }
        Ok(extensions)
    }
}

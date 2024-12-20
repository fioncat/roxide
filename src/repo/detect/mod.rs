pub mod labels;
pub mod stats;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Result;
use glob::Pattern as GlobPattern;

use crate::git;

#[derive(Debug, Clone)]
pub(super) struct Language {
    name: &'static str,
    label: &'static str,

    extensions: Vec<&'static str>,

    // TODO: Support multi-line comments.
    comments: Vec<&'static str>,
}

pub(super) fn builtin_languages() -> Vec<Language> {
    vec![
        Language {
            name: "C",
            label: "c",
            extensions: vec!["c", "h"],
            comments: vec!["//", "/*"],
        },
        Language {
            name: "C++",
            label: "cpp",
            extensions: vec!["cpp", "cc", "C", "hpp"],
            comments: vec!["//", "/*"],
        },
        Language {
            name: "C#",
            label: "csharp",
            extensions: vec!["cs"],
            comments: vec!["//"],
        },
        Language {
            name: "Rust",
            label: "rust",
            extensions: vec!["rs"],
            comments: vec!["//"],
        },
        Language {
            name: "Go",
            label: "go",
            extensions: vec!["go"],
            comments: vec!["//"],
        },
        Language {
            name: "Java",
            label: "java",
            extensions: vec!["java"],
            comments: vec!["//"],
        },
        Language {
            name: "JavaScript",
            label: "js",
            extensions: vec!["js"],
            comments: vec!["//"],
        },
        Language {
            name: "TypeScript",
            label: "ts",
            extensions: vec!["ts"],
            comments: vec!["//"],
        },
        Language {
            name: "Lua",
            label: "lua",
            extensions: vec!["lua"],
            comments: vec!["--"],
        },
        Language {
            name: "Python",
            label: "python",
            extensions: vec!["py"],
            comments: vec!["#"],
        },
        Language {
            name: "HTML",
            label: "html",
            extensions: vec!["html"],
            comments: vec!["<!--"],
        },
        Language {
            name: "CSS",
            label: "css",
            extensions: vec!["css"],
            comments: vec!["/*"],
        },
        Language {
            name: "PHP",
            label: "php",
            extensions: vec!["php"],
            comments: vec!["//", "#", "/*"],
        },
        Language {
            name: "R",
            label: "r",
            extensions: vec!["R"],
            comments: vec!["#"],
        },
        Language {
            name: "Ruby",
            label: "ruby",
            extensions: vec!["rb"],
            comments: vec!["#"],
        },
        Language {
            name: "Perl",
            label: "perl",
            extensions: vec!["pl"],
            comments: vec!["="],
        },
        Language {
            name: "Kotlin",
            label: "kotlin",
            extensions: vec!["kt"],
            comments: vec!["//"],
        },
        Language {
            name: "Scala",
            label: "scala",
            extensions: vec!["scala"],
            comments: vec!["//"],
        },
    ]
}

pub(super) struct Module {
    require: Vec<&'static str>,
    files: Option<Vec<&'static str>>,
    dirs: Option<Vec<&'static str>>,
}

macro_rules! map {
    ($($k:expr => $v:expr),* $(,)?) => {{
        core::convert::From::from([$(($k, $v),)*])
    }};
}

pub(super) fn builtin_modules() -> HashMap<&'static str, Module> {
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
            require: vec!["go"],
            files: Some(vec!["go.mod", "go.work"]),
            dirs: None,
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

pub(super) struct LanguageGroup {
    pub language: Language,
    pub files: Vec<String>,
}

pub(super) fn detect_languages(
    ignores: &[GlobPattern],
    path: &Path,
    languages: &[Language],
) -> Result<Vec<LanguageGroup>> {
    let files = git::list_git_files(path, ignores)?;
    let mut groups_map: HashMap<&str, LanguageGroup> = HashMap::with_capacity(languages.len());

    for file in files {
        let path = PathBuf::from(&file);
        let extension = match path.extension() {
            Some(extension) => match extension.to_str() {
                Some(extension) => extension,
                None => continue,
            },
            None => continue,
        };

        for language in languages {
            let mut is_match = false;
            for lang_extension in language.extensions.iter() {
                if *lang_extension == extension {
                    is_match = true;
                    break;
                }
            }

            if !is_match {
                continue;
            }

            match groups_map.get_mut(language.label) {
                Some(group) => group.files.push(file),
                None => {
                    groups_map.insert(
                        language.label,
                        LanguageGroup {
                            language: language.clone(),
                            files: vec![file],
                        },
                    );
                }
            }
            break;
        }
    }

    let mut groups: Vec<_> = groups_map.into_values().collect();
    groups.sort_unstable_by(|a, b| a.language.label.cmp(b.language.label));
    Ok(groups)
}

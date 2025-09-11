mod parse;

use std::borrow::Cow;
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::ops::Add;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde::Serialize;

use crate::scan::ignore::Ignore;
use crate::scan::{ScanTask, scan_files};
use crate::term::list::ListItem;
use crate::{cursor_up, outputln, report_scan_process};

use super::ScanHandler;

pub async fn get_code_stats(path: PathBuf, ignore: Ignore) -> Result<CodeStats> {
    let stats = CodeStats::default();
    let handler = ScanCodeHandler {
        stats: Mutex::new(stats),
        ignore,
    };
    outputln!("Begin to scan codes");
    let start = Instant::now();
    let result = scan_files(vec![path], handler, false).await;
    cursor_up!();
    let handler = result?;

    let mut stats = handler.stats.into_inner().unwrap();
    stats.elapsed = start.elapsed();

    let mut items: Vec<CodeStatsItem> = stats.data.clone().into_values().collect();
    items.sort_unstable_by(|a, b| b.total.cmp(&a.total));
    stats.items = items;

    Ok(stats)
}

#[derive(Debug, Default)]
pub struct CodeStats {
    pub files_count: usize,

    pub items: Vec<CodeStatsItem>,

    pub elapsed: Duration,

    data: HashMap<&'static str, CodeStatsItem>,
    last_report: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub struct CodeStatsItem {
    pub name: &'static str,
    pub files: u64,
    pub code: u64,
    pub comment: u64,
    pub blank: u64,
    pub total: u64,
}

impl ListItem for CodeStatsItem {
    fn row<'a>(&'a self, title: &str) -> Cow<'a, str> {
        match title {
            "Lang" => Cow::Borrowed(self.name),
            "Files" => self.files.to_string().into(),
            "Code" => self.code.to_string().into(),
            "Comment" => self.comment.to_string().into(),
            "Blank" => self.blank.to_string().into(),
            "Total" => self.total.to_string().into(),
            _ => Cow::Borrowed(""),
        }
    }
}

impl CodeStatsItem {
    fn scan<P>(path: P) -> Result<Option<(&'static str, CodeStatsItem)>>
    where
        P: AsRef<Path>,
    {
        let Some((name, mut parser)) = Self::get_parser(path.as_ref()) else {
            return Ok(None);
        };

        let stats = read_file(path.as_ref(), parser.as_mut())
            .with_context(|| format!("failed to read file {:?}", path.as_ref().display()))?;
        Ok(Some((name, stats)))
    }

    fn get_parser(path: &Path) -> Option<(&'static str, Box<dyn CodeParser>)> {
        let Some(Some(extension)) = path.extension().map(|e| e.to_str()) else {
            let name = path.file_name()?.to_str()?;
            let special = parse::get_special_file(name)?;
            return parse::new_parser(special);
        };
        parse::new_parser(extension)
    }
}

impl Add for CodeStatsItem {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self {
            name: self.name,
            files: self.files + other.files,
            code: self.code + other.code,
            comment: self.comment + other.comment,
            blank: self.blank + other.blank,
            total: self.total + other.total,
        }
    }
}

#[derive(Debug)]
struct ScanCodeHandler {
    stats: Mutex<CodeStats>,
    ignore: Ignore,
}

impl ScanHandler<()> for ScanCodeHandler {
    fn handle_file(&self, file: ScanTask<()>) -> Result<()> {
        let Some((name, item)) = CodeStatsItem::scan(file.path)? else {
            return Ok(());
        };
        let mut stats = self.stats.lock().unwrap();
        let current_item = stats.data.get(name).copied().unwrap_or(CodeStatsItem {
            name,
            ..Default::default()
        });
        let new_item = current_item + item;
        stats.data.insert(name, new_item);
        stats.files_count += 1;

        report_scan_process!(stats);

        Ok(())
    }

    fn should_skip(&self, dir: &Path, _: ()) -> Result<bool> {
        Ok(self.ignore.matched(dir, true))
    }
}

trait CodeParser {
    fn is_comment(&mut self, line: &str) -> bool;
}

fn read_file<P>(path: P, code_parser: &mut dyn CodeParser) -> Result<CodeStatsItem>
where
    P: AsRef<Path>,
{
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut stats = CodeStatsItem {
        files: 1,
        ..Default::default()
    };

    for line in reader.lines() {
        stats.total += 1;
        let line = match line {
            Ok(line) => line,
            Err(e) if e.kind() == io::ErrorKind::InvalidData => {
                // Skip those files that are not valid UTF-8
                return Ok(CodeStatsItem::default());
            }
            Err(e) => return Err(e).context("failed to read file content"),
        };
        let line = line.trim();
        if line.is_empty() {
            stats.blank += 1;
            continue;
        }
        if code_parser.is_comment(line) {
            stats.comment += 1;
            continue;
        }
        stats.code += 1;
    }

    Ok(stats)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::repo::ensure_dir;

    use super::*;

    struct TestFile {
        path: &'static str,
        lines: Vec<&'static str>,
    }

    fn write_test_files(name: &str, files: &[TestFile]) -> PathBuf {
        let dir = PathBuf::from("tests").join(name);
        let _ = fs::remove_dir_all(&dir);
        for file in files {
            let path = dir.join(file.path);
            let base = path.parent().unwrap();
            ensure_dir(base).unwrap();

            let content = file.lines.join("\n");
            fs::write(path, content).unwrap();
        }
        dir
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_code_stats() {
        let files = [
            TestFile {
                path: "src/main.rs",
                lines: vec![
                    "// This is a comment",
                    "",
                    "fn main() {",
                    "    println!(\"Hello, world!\"); // Inline comment",
                    "}",
                ],
            },
            TestFile {
                path: "src/lib.rs",
                lines: vec![
                    "// Another comment",
                    "",
                    "/** This is a doc comment",
                    " * with multiple lines",
                    " */",
                    "fn foo() {",
                    "    // Inside function comment",
                    "    let x = 42;",
                    "}",
                ],
            },
            TestFile {
                path: "python/app.py",
                lines: vec![
                    "# This is a comment",
                    "",
                    "print('Hello, world!')  # Inline comment",
                    "",
                    "    ",
                    "# Another comment",
                    "def foo():",
                    "    pass",
                    "",
                ],
            },
            TestFile {
                path: "config/test.json",
                lines: vec![
                    "{",
                    r#"  "key": "value","#,
                    r#"  "number": 42,"#,
                    r#"  "array": [1, 2, 3]"#,
                    "}",
                    "",
                ],
            },
            TestFile {
                path: "pkg/main.go",
                lines: vec!["// Go comment", "", "package main"],
            },
            TestFile {
                path: "target/test.rs",
                lines: vec!["// This file should be ignored", "fn ignored() {}"],
            },
            TestFile {
                path: "Makefile",
                lines: vec![
                    "# Makefile comment",
                    "",
                    "all:",
                    "\techo 'Building...'",
                    "",
                    "clean:",
                    "\techo 'Cleaning...'",
                ],
            },
        ];
        let path = write_test_files("code_stats", &files);

        let expect_items = vec![
            CodeStatsItem {
                name: "Rust",
                files: 2,
                code: 6,
                comment: 6,
                blank: 2,
                total: 14,
            },
            CodeStatsItem {
                name: "Python",
                files: 1,
                code: 3,
                comment: 2,
                blank: 3,
                total: 8,
            },
            CodeStatsItem {
                name: "Makefile",
                files: 1,
                code: 4,
                comment: 1,
                blank: 2,
                total: 7,
            },
            CodeStatsItem {
                name: "JSON",
                files: 1,
                code: 5,
                comment: 0,
                blank: 0,
                total: 5,
            },
            CodeStatsItem {
                name: "Go",
                files: 1,
                code: 1,
                comment: 1,
                blank: 1,
                total: 3,
            },
        ];

        let ignore = Ignore::parse(&path, ["target"]).unwrap();
        let result = get_code_stats(path, ignore).await.unwrap();
        assert_eq!(result.files_count, 6);
        assert_eq!(result.items, expect_items);
    }
}

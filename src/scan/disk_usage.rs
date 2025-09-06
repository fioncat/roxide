use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::Result;
use pad::PadStr;

use crate::format::{format_bytes, now_millis};
use crate::scan::ignore::Ignore;
use crate::{cursor_up, outputln};

use super::{ScanFile, ScanHandler, scan_files};

pub async fn disk_usage(path: PathBuf, depth: usize, ignore: Ignore) -> Result<DiskUsage> {
    let handler = DiskUsageHandler {
        depth,
        base: path.clone(),
        ignore,
        usage: Mutex::new(DiskUsage::default()),
    };
    outputln!("Begin to scan files");
    let start = Instant::now();
    let result = scan_files(vec![path], handler).await;
    cursor_up!();
    let handler = result?;

    let mut usage = handler.usage.into_inner().unwrap();
    usage.elapsed = start.elapsed();

    let elapsed_secs = usage.elapsed.as_secs_f64();
    let speed = if elapsed_secs > 0.0 {
        usage.files_count as f64 / elapsed_secs
    } else {
        0.0
    };
    usage.speed = speed as u64;

    Ok(usage)
}

#[derive(Debug, Default)]
pub struct DiskUsage {
    pub files_count: usize,
    pub files_usage: u64,

    pub items: BTreeMap<PathBuf, u64>,

    pub elapsed: Duration,
    pub speed: u64,

    last_report: u64,
}

impl DiskUsage {
    pub fn formatted(self) -> Vec<(PathBuf, String)> {
        let total_formatted = format_bytes(self.files_usage);
        let mut usage_width = total_formatted.len();

        let mut formatted_items = Vec::with_capacity(self.items.len());
        for (path, usage) in self.items {
            let formatted = format_bytes(usage);
            if formatted.len() > usage_width {
                usage_width = formatted.len();
            }
            formatted_items.push((path, formatted));
        }

        let mut aligned_items = Vec::with_capacity(formatted_items.len());
        for (path, usage) in formatted_items {
            let aligned = usage.pad_to_width_with_alignment(usage_width, pad::Alignment::Right);
            aligned_items.push((path, aligned));
        }

        let total_aligned =
            total_formatted.pad_to_width_with_alignment(usage_width, pad::Alignment::Right);
        aligned_items.push((PathBuf::from("."), total_aligned));
        aligned_items
    }
}

const REPORT_MILL_SECONDS: u64 = 100;

#[derive(Debug)]
struct DiskUsageHandler {
    depth: usize,
    base: PathBuf,
    ignore: Ignore,

    usage: Mutex<DiskUsage>,
}

impl ScanHandler<()> for DiskUsageHandler {
    fn handle(&self, files: Vec<ScanFile>, _: ()) -> Result<()> {
        if files.is_empty() {
            return Ok(());
        }

        let mut files_count = 0;
        let mut items: BTreeMap<PathBuf, u64> = BTreeMap::new();
        let mut files_usage: u64 = 0;

        for file in files {
            if self.ignore.matched(&file.path, false) {
                continue;
            }
            files_count += 1;
            let usage = file.metadata.len();
            files_usage += usage;
            let path = file.path.strip_prefix(&self.base)?;
            let path = PathBuf::from(path);
            let parts = path.components().collect::<Vec<_>>();
            if parts.len() <= self.depth {
                items.insert(path, usage);
                continue;
            }
            if self.depth == 0 {
                continue;
            }
            let mut root_dir = PathBuf::new();
            for (i, part) in parts.iter().enumerate() {
                if i >= self.depth {
                    break;
                }
                root_dir = root_dir.join(part);
            }
            let current_usage = items.get(&root_dir).copied().unwrap_or_default();
            let usage = current_usage + usage;
            items.insert(root_dir, usage);
        }

        let mut usage = self.usage.lock().unwrap();
        usage.files_count += files_count;
        usage.files_usage += files_usage;
        for (path, dir_usage) in items {
            let current_usage = usage.items.get(&path).copied().unwrap_or_default();
            let new_usage = current_usage + dir_usage;
            usage.items.insert(path, new_usage);
        }

        let now = now_millis();
        let should_report = if usage.last_report == 0 {
            true
        } else {
            let delta = now.saturating_sub(usage.last_report);
            delta > REPORT_MILL_SECONDS
        };
        if should_report {
            cursor_up!();
            outputln!("Scanning {} files", usage.files_count);
            usage.last_report = now;
        }

        Ok(())
    }

    fn should_skip(&self, dir: &Path, _: ()) -> Result<bool> {
        Ok(self.ignore.matched(dir, true))
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::symlink;

    use crate::repo::ensure_dir;

    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn test_disk_usage() {
        struct File {
            path: &'static str,
            size: u64,
        }

        let files = [
            File {
                path: "a/b/c.txt",
                size: 100,
            },
            File {
                path: "a/b/d.txt",
                size: 200,
            },
            File {
                path: "a/e/f.txt",
                size: 300,
            },
            File {
                path: "g/h/i.txt",
                size: 400,
            },
            File {
                path: "g/j/m.txt",
                size: 600,
            },
            File {
                path: "g/k/m.txt",
                size: 700,
            },
            File {
                path: "o/p.txt",
                size: 800,
            },
        ];
        let base_dir = PathBuf::from("tests/disk_usage");
        let _ = fs::remove_dir_all(&base_dir);
        ensure_dir(&base_dir).unwrap();
        let files_count = files.len();
        let mut total_size = 0;
        for case in files {
            total_size += case.size;
            let full_path = base_dir.join(case.path);
            ensure_dir(full_path.parent().unwrap()).unwrap();
            let data = vec![0u8; case.size as usize];
            fs::write(full_path, data).unwrap();
        }

        // The symlink won't be handled now
        symlink(base_dir.join("a/b/c/txt"), base_dir.join("a/b/c_link.txt")).unwrap();

        #[derive(Default)]
        struct Case {
            depth: usize,
            items: BTreeMap<PathBuf, u64>,
            ignore: Ignore,

            files_count: Option<usize>,
            total_size: Option<u64>,
        }

        let cases = [
            Case {
                depth: 0,
                items: BTreeMap::new(),
                ignore: Ignore::default(),
                ..Default::default()
            },
            Case {
                depth: 1,
                items: {
                    let mut m = BTreeMap::new();
                    m.insert(PathBuf::from("a"), 600);
                    m.insert(PathBuf::from("g"), 1700);
                    m.insert(PathBuf::from("o"), 800);
                    m
                },
                ..Default::default()
            },
            Case {
                depth: 2,
                items: {
                    let mut m = BTreeMap::new();
                    m.insert(PathBuf::from("a/b"), 300);
                    m.insert(PathBuf::from("a/e"), 300);
                    m.insert(PathBuf::from("g/h"), 400);
                    m.insert(PathBuf::from("g/j"), 600);
                    m.insert(PathBuf::from("g/k"), 700);
                    m.insert(PathBuf::from("o/p.txt"), 800);
                    m
                },
                ..Default::default()
            },
            Case {
                depth: 1,
                items: {
                    let mut m = BTreeMap::new();
                    m.insert(PathBuf::from("g"), 1700);
                    m.insert(PathBuf::from("o"), 800);
                    m
                },
                files_count: Some(files_count - 3),
                total_size: Some(total_size - 600),
                ignore: Ignore::parse(base_dir.clone(), ["a"]).unwrap(),
            },
            Case {
                depth: 2,
                items: {
                    let mut m = BTreeMap::new();
                    m.insert(PathBuf::from("g/h"), 400);
                    m.insert(PathBuf::from("g/j"), 600);
                    m.insert(PathBuf::from("g/k"), 700);
                    m
                },
                files_count: Some(files_count - 4),
                total_size: Some(total_size - 1400),
                ignore: Ignore::parse(base_dir.clone(), ["a", "o"]).unwrap(),
            },
            Case {
                depth: 4,
                items: {
                    let mut m = BTreeMap::new();
                    m.insert(PathBuf::from("a/b/c.txt"), 100);
                    m.insert(PathBuf::from("a/b/d.txt"), 200);
                    m.insert(PathBuf::from("a/e/f.txt"), 300);
                    m.insert(PathBuf::from("g/h/i.txt"), 400);
                    m.insert(PathBuf::from("g/j/m.txt"), 600);
                    m.insert(PathBuf::from("g/k/m.txt"), 700);
                    m.insert(PathBuf::from("o/p.txt"), 800);
                    m
                },
                ..Default::default()
            },
            Case {
                depth: 4,
                items: BTreeMap::new(),
                ignore: Ignore::parse(base_dir.clone(), ["*.txt"]).unwrap(),
                files_count: Some(0),
                total_size: Some(0),
            },
            Case {
                depth: 1,
                items: {
                    let mut m = BTreeMap::new();
                    m.insert(PathBuf::from("a"), 600);
                    m
                },
                ignore: Ignore::parse(base_dir.clone(), ["o/*.txt", "g/**/*.txt"]).unwrap(),
                files_count: Some(3),
                total_size: Some(600),
            },
        ];

        for case in cases {
            let usage = disk_usage(base_dir.clone(), case.depth, case.ignore)
                .await
                .unwrap();
            match case.files_count {
                Some(c) => assert_eq!(usage.files_count, c),
                None => assert_eq!(usage.files_count, files_count),
            }
            match case.total_size {
                Some(s) => assert_eq!(usage.files_usage, s),
                None => assert_eq!(usage.files_usage, total_size),
            }
            assert_eq!(usage.items, case.items);
        }
    }
}

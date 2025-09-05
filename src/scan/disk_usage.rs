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

        let files_count = files.len();
        let mut items: BTreeMap<PathBuf, u64> = BTreeMap::new();
        let mut files_usage: u64 = 0;

        for file in files {
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

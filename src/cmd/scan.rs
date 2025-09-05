use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

use anyhow::{Context, Result};
use async_trait::async_trait;
use clap::Args;
use pad::PadStr;

use crate::format::{format_bytes, format_elapsed, now_millis};
use crate::ignore::Ignore;
use crate::scan::{ScanFile, ScanHandler, scan_files};
use crate::{cursor_up, outputln};

use super::Command;

const MAX_DEPTH: usize = 9999;

#[derive(Debug, Args)]
pub struct ScanCommand {
    pub path: Option<String>,

    #[clap(short, long, default_value = "1")]
    pub depth: usize,

    #[clap(short, long)]
    pub state: bool,

    #[clap(short = 'I')]
    pub ignore_patterns: Option<Vec<String>>,
}

#[async_trait]
impl Command for ScanCommand {
    async fn run(self) -> Result<()> {
        let path = match self.path {
            Some(p) => PathBuf::from(p),
            None => env::current_dir().context("failed to get current directory")?,
        };
        let ignore = match self.ignore_patterns {
            Some(patterns) => Ignore::parse(&path, patterns)?,
            None => Ignore::default(),
        };
        let depth = if self.depth == 0 {
            MAX_DEPTH
        } else {
            self.depth
        };
        let handler = FileSizeHandler {
            depth,
            base: path.clone(),
            state: Mutex::new(FileSizeState::default()),
            ignore,
        };
        outputln!("Begin to scan files");
        let start = Instant::now();
        let result = scan_files(vec![path], handler).await;
        cursor_up!();
        let handler = result?;
        let state = handler.state.lock().unwrap();

        if self.state {
            let end = Instant::now();
            let elapsed = end - start;
            let elapsed_secs = elapsed.as_secs_f64();
            let speed = if elapsed_secs > 0.0 {
                state.files_count as f64 / elapsed_secs
            } else {
                0.0
            };
            outputln!(
                "Total {} files, took: {}, speed: {} files/s",
                state.files_count,
                format_elapsed(elapsed),
                speed as u64,
            );
        }

        let mut items: Vec<_> = state.dirs_size.iter().collect();
        items.sort_unstable_by(|(a, _), (b, _)| a.cmp(b));

        let mut formated_items = Vec::with_capacity(items.len());
        let total_formated = format_bytes(state.files_size);
        let mut size_width = total_formated.len();
        for (path, size) in items {
            let formated = format_bytes(*size);
            if formated.len() > size_width {
                size_width = formated.len();
            }
            formated_items.push((path, formated));
        }

        for (path, size) in formated_items {
            let size = size.pad_to_width_with_alignment(size_width, pad::Alignment::Right);
            outputln!("{size}  {}", path.display());
        }
        outputln!(
            "{}  .",
            total_formated.pad_to_width_with_alignment(size_width, pad::Alignment::Right)
        );

        Ok(())
    }
}

const REPORT_MILL_SECONDS: u64 = 100;

#[derive(Debug, Default)]
struct FileSizeHandler {
    depth: usize,
    base: PathBuf,
    state: Mutex<FileSizeState>,
    ignore: Ignore,
}

#[derive(Debug, Default)]
struct FileSizeState {
    files_count: usize,
    dirs_size: HashMap<PathBuf, u64>,
    files_size: u64,
    last_report: u64,
}

impl ScanHandler<()> for FileSizeHandler {
    fn handle(&self, files: Vec<ScanFile>, _: ()) -> Result<()> {
        if files.is_empty() {
            return Ok(());
        }

        let files_count = files.len();
        let mut dirs_size: HashMap<PathBuf, u64> = HashMap::new();
        let mut files_size: u64 = 0;

        for file in files {
            let size = file.metadata.len();
            files_size += size;
            let path = file.path.strip_prefix(&self.base)?;
            let path = PathBuf::from(path);
            let parts = path.components().collect::<Vec<_>>();
            if parts.len() <= self.depth {
                dirs_size.insert(path, size);
                continue;
            }
            let mut root_dir = PathBuf::new();
            for (i, part) in parts.iter().enumerate() {
                if i >= self.depth {
                    break;
                }
                root_dir = root_dir.join(part);
            }
            let current_size = dirs_size.get(&root_dir).copied().unwrap_or_default();
            let size = current_size + size;
            dirs_size.insert(root_dir, size);
        }

        let mut state = self.state.lock().unwrap();
        state.files_count += files_count;
        state.files_size += files_size;
        for (path, size) in dirs_size {
            let current_size = state.dirs_size.get(&path).copied().unwrap_or_default();
            let size = current_size + size;
            state.dirs_size.insert(path, size);
        }

        let now = now_millis();
        let shoud_report = if state.last_report == 0 {
            true
        } else {
            let delta = now.saturating_sub(state.last_report);
            delta > REPORT_MILL_SECONDS
        };
        if shoud_report {
            cursor_up!();
            outputln!("Scanning {} files", state.files_count);
            state.last_report = now;
        }

        Ok(())
    }

    fn should_skip(&self, dir: &Path, _: ()) -> Result<bool> {
        Ok(self.ignore.matched(dir, true))
    }
}

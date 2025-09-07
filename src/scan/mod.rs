pub mod code_stats;
pub mod disk_usage;
pub mod ignore;

use std::fmt::Debug;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use crossbeam::channel::{self, Receiver, Sender};
use crossbeam::select;

use crate::debug;

#[derive(Debug, Clone)]
pub struct ScanTask<T: Send + Sync + Clone + Debug> {
    pub path: PathBuf,
    pub metadata: fs::Metadata,
    pub data: T,
}

impl<T: Send + Sync + Clone + Debug> ScanTask<T> {
    pub fn new(path: PathBuf, data: T) -> Result<Self> {
        let metadata = path
            .metadata()
            .with_context(|| format!("failed to get metadata for path: {:?}", path.display()))?;
        Ok(Self {
            path,
            metadata,
            data,
        })
    }
}

pub trait ScanHandler<T: Send + Sync + Clone + Debug>: Send + Sync {
    fn handle_files(&self, _files: Vec<(PathBuf, fs::Metadata)>, _data: T) -> Result<()> {
        unreachable!("handle_files should not be called when not in compact mode");
    }

    fn handle_file(&self, _task: ScanTask<T>) -> Result<()> {
        unreachable!("handle_file should not be called when in compact mode");
    }

    fn should_skip(&self, dir: &Path, data: T) -> Result<bool>;
}

struct Dispatcher<T: Send + Sync + Clone + Debug, I: IntoIterator<Item = ScanTask<T>>> {
    bootstrap_tasks: Option<I>,

    tasks: Sender<ScanTask<T>>,
    stops: Vec<Sender<()>>,

    results: Receiver<Result<Vec<ScanTask<T>>>>,

    dispatched: usize,
}

impl<T: Send + Sync + Clone + Debug, I: IntoIterator<Item = ScanTask<T>>> Dispatcher<T, I> {
    fn wait(&mut self) -> Result<()> {
        debug!("[scan] Dispatcher start");
        loop {
            self.dispatch()?;
            if self.dispatched == 0 {
                debug!("[scan] Dispatcher: no more task to handle, stop");
                return Ok(());
            }
        }
    }

    fn dispatch(&mut self) -> Result<()> {
        if let Some(tasks) = self.bootstrap_tasks.take() {
            for task in tasks {
                debug!("[scan] Dispatcher: send bootstrap task {task:?} to workers");
                self.tasks.send(task).unwrap();
                self.dispatched += 1;
            }
        }

        let sub_tasks = self.results.recv().unwrap()?;
        self.dispatched -= 1;

        for task in sub_tasks {
            debug!("[scan] Dispatcher: send sub task {task:?} to workers");
            self.tasks.send(task).unwrap();
            self.dispatched += 1;
        }

        debug!("[scan] Dispatche done, dispatched: {}", self.dispatched);
        Ok(())
    }

    fn stop(&self) {
        for stop in self.stops.iter() {
            stop.send(()).unwrap();
        }
    }
}

struct Worker<T: Send + Sync + Clone + Debug> {
    index: usize,

    tasks: Receiver<ScanTask<T>>,
    stop: Receiver<()>,

    handler: Arc<dyn ScanHandler<T>>,

    results: Sender<Result<Vec<ScanTask<T>>>>,

    compact: bool,
}

impl<T: Send + Sync + Clone + Debug> Worker<T> {
    fn run(&mut self) {
        debug!("[scan] Worker {} start", self.index);
        loop {
            select! {
                recv(self.tasks) -> task => {
                    let path = task.unwrap();
                    let result = self.handle(path);
                    self.results.send(result).unwrap();
                },
                recv(self.stop) -> _ => {
                    debug!("[scan] Worker {} stop", self.index);
                    return;
                },
            }
        }
    }

    fn handle(&self, task: ScanTask<T>) -> Result<Vec<ScanTask<T>>> {
        debug!(
            "[scan] Worker {} begin to handle task: {task:?}",
            self.index
        );
        if !task.path.exists() {
            bail!("path does not exist: {:?}", task.path.display());
        }

        if task.metadata.is_file() {
            debug!(
                "[scan] Worker {}: task is a file, handle it directly",
                self.index
            );
            if self.compact {
                self.handler
                    .handle_files(vec![(task.path, task.metadata)], task.data)?;
            } else {
                self.handler.handle_file(task)?;
            }
            return Ok(vec![]);
        }

        if !task.metadata.is_dir() {
            // TODO: Handle symlink
            debug!("[scan] Worker {}: task is a symlink, skip it", self.index);
            return Ok(vec![]);
        }

        let mut sub_tasks = vec![];
        let mut sub_files = vec![];
        let ents = fs::read_dir(&task.path)
            .with_context(|| format!("failed to read dir {:?}", task.path.display()))?;
        for ent in ents {
            let ent = ent.with_context(|| {
                format!("failed to read dir entry in {:?}", task.path.display())
            })?;
            let sub_path = task.path.join(ent.file_name());
            let sub_metadata = ent.metadata().with_context(|| {
                format!("failed to get metadata for path: {:?}", sub_path.display())
            })?;

            if sub_metadata.is_file() {
                debug!(
                    "[scan] Worker {}: task sub entry {:?} is a file",
                    self.index,
                    sub_path.display()
                );
                sub_files.push((sub_path, sub_metadata));
                continue;
            }

            if !sub_metadata.is_dir() {
                // TODO: Handle symlink
                debug!(
                    "[scan] Worker {}: task sub entry {:?} is a symlink, skip",
                    self.index,
                    sub_path.display()
                );
                continue;
            }

            if self.handler.should_skip(&sub_path, task.data.clone())? {
                debug!(
                    "[scan] Worker {}: task sub dir {:?} ignored by handler",
                    self.index,
                    sub_path.display()
                );
                continue;
            }

            debug!(
                "[scan] Worker {}: task sub dir {:?} will continue to read",
                self.index,
                sub_path.display()
            );
            sub_tasks.push(ScanTask {
                path: sub_path,
                metadata: sub_metadata,
                data: task.data.clone(),
            });
        }

        if sub_files.is_empty() {
            debug!("[scan] Worker {}: no file found, handle done", self.index);
            return Ok(sub_tasks);
        }

        if self.compact {
            debug!(
                "[scan] Worker {} is in compact mode, handle files {sub_files:?} directly",
                self.index
            );
            self.handler.handle_files(sub_files, task.data.clone())?;
        } else {
            debug!(
                "[scan] Worker {} not in compact mode, send files {sub_files:?} to dispatcher to let other workers handle",
                self.index
            );
            sub_tasks.extend(sub_files.into_iter().map(|(path, metadata)| ScanTask {
                path,
                metadata,
                data: task.data.clone(),
            }));
        }

        debug!("[scan] Worker {}: handle done", self.index);
        Ok(sub_tasks)
    }
}

pub async fn scan_files<I, H, S>(paths: I, handler: H, compact: bool) -> Result<H>
where
    I: IntoIterator<Item = S>,
    H: ScanHandler<()> + std::fmt::Debug + 'static,
    S: AsRef<Path>,
{
    let mut tasks = vec![];
    for path in paths {
        let task = ScanTask::new(path.as_ref().to_path_buf(), ())?;
        tasks.push(task);
    }
    scan_files_with_data(tasks, handler, compact).await
}

pub async fn scan_files_with_data<I, H, T>(tasks: I, handler: H, compact: bool) -> Result<H>
where
    I: IntoIterator<Item = ScanTask<T>> + Debug,
    H: ScanHandler<T> + Debug + 'static,
    T: Send + Sync + Clone + Debug + 'static,
{
    debug!("[scan] Begin to scan tasks: {tasks:?}");
    let handler = Arc::new(handler);

    let worker_count = num_cpus::get();
    assert!(worker_count > 0);
    debug!("[scan] Worker count: {worker_count}");

    let (tasks_tx, tasks_rx) = channel::unbounded();
    let (stop_tx, stop_rx) = channel::unbounded();
    let (results_tx, results_rx) = channel::unbounded();

    let mut stops = Vec::with_capacity(worker_count);
    let mut handlers = Vec::with_capacity(worker_count);
    for i in 0..worker_count {
        let mut worker = Worker {
            index: i,
            tasks: tasks_rx.clone(),
            stop: stop_rx.clone(),
            handler: handler.clone(),
            results: results_tx.clone(),
            compact,
        };
        stops.push(stop_tx.clone());

        let handler = tokio::spawn(async move {
            worker.run();
        });
        handlers.push(handler);
    }

    let mut dispatcher = Dispatcher {
        bootstrap_tasks: Some(tasks),
        tasks: tasks_tx,
        stops,
        results: results_rx,
        dispatched: 0,
    };
    let result = dispatcher.wait();
    dispatcher.stop();
    debug!("[scan] Dispatch done, wait workers stop");
    for handler in handlers {
        handler.await.unwrap();
    }
    result?;

    let handler = Arc::try_unwrap(handler).unwrap();
    debug!("[scan] Workers stopped, new handler: {handler:?}");
    Ok(handler)
}

pub const REPORT_MILL_SECONDS: u64 = 100;

#[macro_export]
macro_rules! report_scan_process {
    ($self:ident) => {
        let now = $crate::format::now_millis();
        let should_report = if $self.last_report == 0 {
            true
        } else {
            let delta = now.saturating_sub($self.last_report);
            delta > $crate::scan::REPORT_MILL_SECONDS
        };
        if should_report {
            $crate::cursor_up!();
            $crate::outputln!("Scanning {} files", $self.files_count);
            $self.last_report = now;
        }
    };
}

#[macro_export]
macro_rules! report_scan_process_done {
    ($self:ident) => {
        $crate::outputln!(
            "Total {} files, took: {}, speed: {} files/s",
            $self.files_count,
            $crate::format::format_elapsed($self.elapsed),
            $crate::scan::get_handle_speed($self.elapsed, $self.files_count)
        );
    };
}

pub fn get_handle_speed(elapsed: Duration, files_count: usize) -> u64 {
    let elapsed_secs = elapsed.as_secs_f64();
    let speed = if elapsed_secs > 0.0 {
        files_count as f64 / elapsed_secs
    } else {
        0.0
    };
    speed as u64
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[derive(Debug)]
    struct TestHandler {
        files: Mutex<Vec<PathBuf>>,
        compact: bool,
    }

    impl ScanHandler<Arc<String>> for TestHandler {
        fn handle_files(
            &self,
            files: Vec<(PathBuf, fs::Metadata)>,
            data: Arc<String>,
        ) -> Result<()> {
            if !self.compact {
                unreachable!();
            }
            let root = PathBuf::from("tests").join(data.as_str());
            let root = fs::canonicalize(root).unwrap();

            let mut all_files = self.files.lock().unwrap();
            for (path, _) in files {
                assert!(path.starts_with(&root));
                all_files.push(path);
            }

            Ok(())
        }

        fn handle_file(&self, task: ScanTask<Arc<String>>) -> Result<()> {
            if self.compact {
                unreachable!();
            }
            let root = PathBuf::from("tests").join(task.data.as_str());
            let root = fs::canonicalize(root).unwrap();

            let mut all_files = self.files.lock().unwrap();
            assert!(task.path.starts_with(&root));
            all_files.push(task.path);
            Ok(())
        }

        fn should_skip(&self, _dir: &Path, data: Arc<String>) -> Result<bool> {
            if data.as_str() == "scan_skip" {
                return Ok(true);
            }
            Ok(false)
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_scan() {
        let paths = [
            "scan_dir0",
            "scan_dir0/subdir0",
            "scan_dir0/subdir1",
            "scan_dir0/subdir2",
            "scan_dir0/empty",
            "scan_dir0/subdir0/subdir3",
            "scan_dir0/subdir0/file0.txt",
            "scan_dir0/subdir0/file1.txt",
            "scan_dir0/subdir0/subdir3/file2.txt",
            "scan_dir0/subdir0/subdir3/file3.txt",
            "scan_dir0/subdir1/file4.txt",
            "scan_dir0/subdir2/file5.txt",
            "scan_dir0/file6.txt",
            "scan_dir0/file7.txt",
            "scan_dir1",
            "scan_dir1/file0.txt",
            "scan_dir1/file1.txt",
            "scan_dir1/file2.txt",
            "scan_skip",
            "scan_skip/file0.txt",
            "scan_skip/subdir",
            "scan_skip/subdir/file1.text",
            "scan_skip/subdir/file2.text",
        ];
        let _ = fs::remove_dir_all("tests/scan_dir0");
        let _ = fs::remove_dir_all("tests/scan_dir1");
        let _ = fs::remove_dir_all("tests/scan_skip");
        let mut expected = vec![];
        for path in paths {
            let path = PathBuf::from("tests").join(path);
            if path.ends_with(".txt") {
                fs::write(&path, b"test content").unwrap();

                if path.starts_with("tests/scan_skip/subdir") {
                    continue;
                }

                let path = fs::canonicalize(path).unwrap();
                expected.push(path);
            } else {
                fs::create_dir_all(&path).unwrap();
            }
        }

        let handler = TestHandler {
            files: Mutex::new(vec![]),
            compact: false,
        };

        let tasks = [
            ScanTask::new(
                fs::canonicalize("tests/scan_dir0").unwrap(),
                Arc::new("scan_dir0".to_string()),
            )
            .unwrap(),
            ScanTask::new(
                fs::canonicalize("tests/scan_dir1").unwrap(),
                Arc::new("scan_dir1".to_string()),
            )
            .unwrap(),
            ScanTask::new(
                fs::canonicalize("tests/scan_skip").unwrap(),
                Arc::new("scan_skip".to_string()),
            )
            .unwrap(),
        ];
        let handler = scan_files_with_data(tasks.clone(), handler, false)
            .await
            .unwrap();
        let results = handler.files.into_inner().unwrap();
        assert_eq!(results.as_ref(), expected);

        let handler = TestHandler {
            files: Mutex::new(vec![]),
            compact: true,
        };
        let handler = scan_files_with_data(tasks, handler, true).await.unwrap();
        let results = handler.files.lock().unwrap();
        assert_eq!(results.as_ref(), expected);
    }
}

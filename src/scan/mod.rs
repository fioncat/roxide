pub mod code_stats;
pub mod disk_usage;
pub mod ignore;

use std::fmt::Debug;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use crossbeam::channel::{self, Receiver, Sender};
use crossbeam::select;

use crate::debug;

pub struct ScanFile {
    pub path: PathBuf,
    pub metadata: fs::Metadata,
}

pub trait ScanHandler<T: Send + Sync + Clone>: Send + Sync {
    fn handle(&self, files: Vec<ScanFile>, data: T) -> Result<()>;

    fn should_skip(&self, dir: &Path, data: T) -> Result<bool>;
}

#[derive(Debug)]
pub struct Task<T: Send + Sync + Clone + Debug> {
    pub path: PathBuf,
    pub data: T,
}

struct Dispatcher<T: Send + Sync + Clone + Debug, I: IntoIterator<Item = Task<T>>> {
    bootstrap_tasks: Option<I>,

    tasks: Sender<Task<T>>,
    stops: Vec<Sender<()>>,

    results: Receiver<Result<Vec<Task<T>>>>,

    dispatched: usize,
}

impl<T: Send + Sync + Clone + Debug, I: IntoIterator<Item = Task<T>>> Dispatcher<T, I> {
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

    tasks: Receiver<Task<T>>,
    stop: Receiver<()>,

    handler: Arc<dyn ScanHandler<T>>,

    results: Sender<Result<Vec<Task<T>>>>,
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

    fn handle(&self, task: Task<T>) -> Result<Vec<Task<T>>> {
        debug!(
            "[scan] Worker {} begin to handle task: {task:?}",
            self.index
        );
        let Task { path, data } = task;
        if !path.exists() {
            bail!("path does not exist: {:?}", path.display());
        }

        let metadata = fs::metadata(&path)
            .with_context(|| format!("failed to get metadata for path: {:?}", path.display()))?;

        if metadata.is_file() {
            debug!(
                "[scan] Worker {}: task is a file, handle it directly",
                self.index
            );
            self.handler
                .handle(vec![ScanFile { path, metadata }], data.clone())?;
            return Ok(vec![]);
        }

        if !metadata.is_dir() {
            // TODO: Handle symlink
            debug!("[scan] Worker {}: task is a symlink, skip it", self.index);
            return Ok(vec![]);
        }

        let mut sub_dirs = vec![];
        let mut sub_files = vec![];
        let ents = fs::read_dir(&path)
            .with_context(|| format!("failed to read dir {:?}", path.display()))?;
        for ent in ents {
            let ent =
                ent.with_context(|| format!("failed to read dir entry in {:?}", path.display()))?;
            let sub_path = path.join(ent.file_name());
            let sub_metadata = ent.metadata().with_context(|| {
                format!("failed to get metadata for path: {:?}", sub_path.display())
            })?;

            if sub_metadata.is_file() {
                debug!(
                    "[scan] Worker {}: task sub entry {:?} is a file, add to handle list",
                    self.index,
                    sub_path.display()
                );
                sub_files.push(ScanFile {
                    path: sub_path,
                    metadata: sub_metadata,
                });
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

            if self.handler.should_skip(&sub_path, data.clone())? {
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
            sub_dirs.push(Task {
                path: sub_path,
                data: data.clone(),
            });
        }

        if !sub_files.is_empty() {
            debug!("[scan] Worker {}: handle files", self.index);
            self.handler.handle(sub_files, data.clone())?;
        }

        debug!("[scan] Worker {}: handle done", self.index);
        Ok(sub_dirs)
    }
}

pub async fn scan_files<I, H, S>(paths: I, handler: H) -> Result<H>
where
    I: IntoIterator<Item = S>,
    H: ScanHandler<()> + std::fmt::Debug + 'static,
    S: AsRef<Path>,
{
    let tasks = paths
        .into_iter()
        .map(|path| Task {
            path: PathBuf::from(path.as_ref()),
            data: (),
        })
        .collect::<Vec<_>>();
    scan_files_with_data(tasks, handler).await
}

pub async fn scan_files_with_data<I, H, T>(tasks: I, handler: H) -> Result<H>
where
    I: IntoIterator<Item = Task<T>> + Debug,
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

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[derive(Debug)]
    struct TestHandler {
        files: Mutex<Vec<PathBuf>>,
    }

    impl ScanHandler<Arc<String>> for TestHandler {
        fn handle(&self, files: Vec<ScanFile>, data: Arc<String>) -> Result<()> {
            let root = PathBuf::from("tests").join(data.as_str());
            let root = fs::canonicalize(root).unwrap();

            let mut all_files = self.files.lock().unwrap();
            for file in files {
                assert!(file.path.starts_with(&root));
                all_files.push(file.path);
            }
            Ok(())
        }

        fn should_skip(&self, _dir: &Path, data: Arc<String>) -> Result<bool> {
            if data.as_str() == "scan_skip" {
                return Ok(true);
            }
            Ok(false)
        }
    }

    #[tokio::test(flavor = "multi_thread")] // We must use multi thread when using crossbeam
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
        };

        let tasks = [
            Task {
                path: fs::canonicalize("tests/scan_dir0").unwrap(),
                data: Arc::new("scan_dir0".to_string()),
            },
            Task {
                path: fs::canonicalize("tests/scan_dir1").unwrap(),
                data: Arc::new("scan_dir1".to_string()),
            },
            Task {
                path: fs::canonicalize("tests/scan_skip").unwrap(),
                data: Arc::new("scan_skip".to_string()),
            },
        ];
        let handler = scan_files_with_data(tasks, handler).await.unwrap();
        let results = handler.files.lock().unwrap();
        assert_eq!(results.as_ref(), expected);
    }
}

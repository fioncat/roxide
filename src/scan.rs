use std::fs;
use std::mem::take;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, bail};
use tokio::sync::{broadcast, mpsc};

pub struct ScanItem {
    pub path: PathBuf,
    pub metadata: fs::Metadata,
}

pub trait ScanTask: Send + Sync {
    fn run(&self, files: Vec<ScanItem>) -> Result<()>;
}

struct Dispatcher {
    workers_count: usize,

    to_handle: Vec<PathBuf>,

    notify: broadcast::Sender<bool>,
    work_queue: Arc<Mutex<Vec<Option<PathBuf>>>>,

    feedback: mpsc::Receiver<Result<Vec<PathBuf>>>,
}

impl Dispatcher {
    async fn wait(&mut self) -> Result<()> {
        let mut result = Ok(());
        loop {
            match self.dispatch().await {
                Ok(true) => {}
                Ok(false) => break,
                Err(e) => {
                    result = Err(e);
                    break;
                }
            }
        }
        self.notify.send(false).unwrap();
        result
    }

    async fn dispatch(&mut self) -> Result<bool> {
        if self.to_handle.is_empty() {
            return Ok(false);
        }

        let mut dispatched: usize = 0;
        {
            let mut work_queue = self.work_queue.lock().unwrap();
            for i in 0..self.workers_count {
                let Some(path) = self.to_handle.pop() else {
                    break;
                };
                dispatched += 1;
                work_queue[i] = Some(path);
            }
        }

        self.notify.send(true).unwrap();

        let mut err = None;
        for _ in 0..dispatched {
            let result = self.feedback.recv().await.unwrap();
            match result {
                Ok(dirs) => self.to_handle.extend(dirs),
                Err(e) => err = Some(e),
            }
        }
        if let Some(e) = err {
            return Err(e);
        }

        Ok(true)
    }
}

struct Worker {
    index: usize,

    task: Arc<dyn ScanTask>,

    notify: broadcast::Receiver<bool>,
    work_queue: Arc<Mutex<Vec<Option<PathBuf>>>>,

    feedback: mpsc::Sender<Result<Vec<PathBuf>>>,
}

impl Worker {
    async fn run(&mut self) {
        loop {
            let shoud_continue = self.notify.recv().await.unwrap();
            if !shoud_continue {
                break;
            }

            let path = {
                let mut work_queue = self.work_queue.lock().unwrap();
                take(&mut work_queue[self.index])
            };

            if let Some(path) = path {
                let result = self.handle(path);
                self.feedback.send(result).await.unwrap();
            }
        }
    }

    fn handle(&self, path: PathBuf) -> Result<Vec<PathBuf>> {
        if !path.exists() {
            bail!("path {path:?} does not exist");
        }
        let real_path = fs::canonicalize(&path)
            .with_context(|| format!("failed to get real path for {path:?}"))?;
        if !real_path.exists() {
            bail!("path {real_path:?} does not exist");
        }

        let metadata = fs::metadata(&real_path)
            .with_context(|| format!("failed to get metadata for {real_path:?}"))?;

        if metadata.is_file() {
            self.task.run(vec![ScanItem {
                path: real_path,
                metadata,
            }])?;
            return Ok(vec![]);
        }

        let mut sub_dirs = vec![];
        let mut files = vec![];
        let ents = fs::read_dir(&real_path)
            .with_context(|| format!("failed to read directory {real_path:?}"))?;
        for ent in ents {
            let ent = ent.with_context(|| format!("failed to read entry in {real_path:?}"))?;
            let name = ent.file_name();
            let sub_path = real_path.join(&name);
            let sub_metadata = ent
                .metadata()
                .with_context(|| format!("failed to get metadata for {sub_path:?}"))?;

            if sub_metadata.is_dir() {
                sub_dirs.push(sub_path);
            } else if sub_metadata.is_file() {
                files.push(ScanItem {
                    path: sub_path,
                    metadata: sub_metadata,
                });
            }
        }

        if !files.is_empty() {
            self.task.run(files)?;
        }

        Ok(sub_dirs)
    }
}

pub async fn scan_files<T, I, P>(task: T, files: I) -> Result<T>
where
    T: ScanTask + std::fmt::Debug + 'static,
    I: IntoIterator<Item = P>,
    P: AsRef<Path>,
{
    let files = files
        .into_iter()
        .map(|p| p.as_ref().to_path_buf())
        .collect::<Vec<_>>();
    let task = Arc::new(task);

    let workers_count = num_cpus::get();
    assert!(workers_count > 0);

    let (notify_tx, _) = broadcast::channel(workers_count);
    let (feedback_tx, feedback_rx) = mpsc::channel(workers_count);

    let work_queue = Arc::new(Mutex::new(vec![None; workers_count]));

    let mut dispatcher = Dispatcher {
        workers_count,
        to_handle: files,
        notify: notify_tx,
        work_queue: work_queue.clone(),
        feedback: feedback_rx,
    };

    let mut handlers = vec![];
    for index in 0..workers_count {
        let mut worker = Worker {
            index,
            task: task.clone(),
            notify: dispatcher.notify.subscribe(),
            work_queue: work_queue.clone(),
            feedback: feedback_tx.clone(),
        };
        let handler = tokio::spawn(async move {
            worker.run().await;
        });
        handlers.push(handler);
    }

    let result = dispatcher.wait().await;
    for handler in handlers {
        handler.await.unwrap();
    }
    result?;

    let task = Arc::try_unwrap(task).expect("task should be able to unwrap");
    Ok(task)
}

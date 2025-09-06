use std::sync::{Arc, Mutex};

use anyhow::Result;
use console::style;
use tokio::sync::mpsc;

use crate::term::track::{Report, Tracker};
use crate::{debug, outputln};

#[async_trait::async_trait]
pub trait Task<R: Send> {
    fn name(&self) -> Arc<String>;

    async fn run(&self) -> Result<R>;
}

pub async fn run<T, R>(desc: &str, tasks: Vec<T>) -> Result<Vec<R>>
where
    R: Send + 'static,
    T: Task<R> + Send + 'static,
{
    debug!("[batch] Run {desc:?} with {} tasks", tasks.len());
    if tasks.is_empty() {
        debug!("[batch] No tasks to run, return immediately");
        return Ok(vec![]);
    }

    // Set the number of workers to the number of cpu cores to maximize the use of
    // multicore cpu.
    // Here num_cpus can guarantee that the number of cores returned is greater
    // than 0.
    let worker_len = num_cpus::get();
    assert_ne!(worker_len, 0);

    let tasks_len = tasks.len();
    let mut tasks_queue = Vec::with_capacity(tasks_len);
    for (i, task) in tasks.into_iter().enumerate() {
        tasks_queue.push((i, task));
    }
    let tasks_queue = Arc::new(Mutex::new(tasks_queue));

    // This mpsc channel is used to receive worker progress reports. Including task
    // execution progress and completion status.
    let (report_tx, report_rx) = mpsc::channel::<Report<R>>(tasks_len);

    let title = style(format!("{desc} with {worker_len} workers:"))
        .bold()
        .cyan()
        .underlined();
    outputln!("{title}\n");

    let mut handlers = Vec::with_capacity(worker_len);
    for worker in 0..worker_len {
        let report_tx = report_tx.clone();
        let tasks_queue = tasks_queue.clone();
        let handler = tokio::spawn(async move {
            debug!("[batch] Worker {worker} started");
            loop {
                let task = {
                    let mut queue = tasks_queue.lock().unwrap();
                    queue.pop()
                };
                let Some((i, task)) = task else {
                    debug!("[batch] Worker {worker} done");
                    break;
                };

                let name = task.name();
                debug!("[batch] Worker {worker} running task {i}: {name}");
                report_tx.send(Report::Running(i, name)).await.unwrap();
                let result = task.run().await;
                debug!(
                    "[batch] Worker {worker} finished task {i}: {}, is_ok: {}",
                    task.name(),
                    result.is_ok()
                );
                report_tx.send(Report::Done(i, result)).await.unwrap();
            }
        });
        handlers.push(handler);
    }

    debug!("[batch] All workers started");
    let tracker = Tracker::new(desc, tasks_len);
    let results = tracker.wait(report_rx).await;
    for handler in handlers {
        handler.await.unwrap();
    }
    debug!("[batch] All workers done");
    results
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use anyhow::bail;
    use tokio::time;

    use super::*;

    struct TestTask {
        idx: usize,
    }

    #[async_trait::async_trait]
    impl Task<usize> for TestTask {
        fn name(&self) -> Arc<String> {
            Arc::new(format!("Task-{}", self.idx))
        }

        async fn run(&self) -> Result<usize> {
            time::sleep(Duration::from_millis(100)).await;
            Ok(self.idx)
        }
    }

    #[tokio::test]
    async fn test_run() {
        const COUNT: usize = 100;
        let mut tasks = Vec::with_capacity(COUNT);
        for i in 0..COUNT {
            let task = TestTask { idx: i };
            tasks.push(task);
        }

        let results = run("Test", tasks).await.unwrap();
        assert_eq!(results.len(), COUNT);
        for (i, res) in results.iter().enumerate() {
            assert_eq!(*res, i);
        }
    }

    struct FailTask {
        idx: usize,
    }

    #[async_trait::async_trait]
    impl Task<()> for FailTask {
        fn name(&self) -> Arc<String> {
            Arc::new(format!("FailTask-{}", self.idx))
        }

        async fn run(&self) -> Result<()> {
            time::sleep(Duration::from_millis(100)).await;
            if self.idx == 50 {
                bail!("Intentional failure");
            }
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_run_fail() {
        const COUNT: usize = 100;
        let mut tasks = Vec::with_capacity(COUNT);
        for i in 0..COUNT {
            let task = FailTask { idx: i };
            tasks.push(task);
        }
        let result = run("Test", tasks).await;
        assert_eq!(result.unwrap_err().to_string(), "Test task failed");
    }
}

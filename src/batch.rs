#![allow(dead_code)]

use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{bail, Result};
use console::style;

use crate::{stderr, term};

/// `Task` is used to represent a concurrent task that needs to be executed.
///
/// Generally pass a vector of objects that implement this trait to [`run`] for batch
/// concurrent execution of tasks.
pub trait Task<R: Send> {
    /// Represents the function that needs to be executed concurrently for a task.
    ///
    /// # Attentions
    ///
    /// Please note that this function will be concurrently called by different
    /// threads, and the caller needs to ensure thread safety.
    fn run(&self) -> Result<R>;
}

/// Used by [`Tracker`] for reporting the execution results of tasks from worker
/// threads to the main thread.
enum Report<R> {
    /// Indicates that the task is still in progress, where `usize` is the current
    /// task index and `String` is the name of the current task.
    Running(usize, String),

    /// Indicates that the task has been completed. `usize` is the current task
    /// index, and `Result<R>` represents the execution result of the task.
    Done(usize, Result<R>),
}

/// `Tracker` is used to track the execution status of batch tasks and output the
/// status to the terminal in real-time.
struct Tracker<R> {
    /// total tasks length.
    total: usize,
    /// padding size for `total`.
    total_pad: usize,

    /// running tasks.
    running: Vec<(usize, String)>,
    /// done tasks.
    done: Vec<(usize, Result<R>)>,

    /// task description, such as "Run"
    desc: String,
    /// task description without style
    desc_pure: String,
    /// task description string length
    desc_size: usize,
    /// task description head, such as "Running"
    desc_head: String,

    ok_count: usize,
    fail_count: usize,

    /// If tasks failed, display their error messages.
    show_fail: bool,
    fail_message: Option<Vec<(String, String)>>,
}

impl<R> Tracker<R> {
    const SPACE: &'static str = " ";
    const SEP: &'static str = ", ";
    const OMIT: &'static str = ", ...";

    const SPACE_SIZE: usize = Self::SPACE.len();
    const SEP_SIZE: usize = Self::SEP.len();
    const OMIT_SIZE: usize = Self::OMIT.len();

    /// Create a Tracker, call [`Tracker::wait`] later to start tracking.
    ///
    /// # Arguments
    ///
    /// * `desc` - A descriptive string for the task, which will be printed in the
    ///   terminal.
    /// * `total` - The expected number of tasks. Tracking will stop when the number
    ///   of completed tasks reaches this value.
    /// * `show_fail` - If `true`, show error messages for tasks after they fail.
    fn new(desc: &str, total: usize, show_fail: bool) -> Tracker<R> {
        let desc_pure = String::from(desc);
        let desc = style(desc).cyan().bold().to_string();
        let desc_size = Self::get_size(&desc);

        let total_pad = total.to_string().chars().count();

        Tracker {
            total,
            total_pad,
            running: Vec::with_capacity(total),
            done: Vec::with_capacity(total),
            desc,
            desc_pure,
            desc_size,
            desc_head: " ".repeat(desc_size),
            ok_count: 0,
            fail_count: 0,
            show_fail,
            fail_message: None,
        }
    }

    /// Receive the execution results of tasks from `rx` and track them in real-time.
    /// This function will block the main thread until the expected number of
    /// completed tasks is reached and return the task execution results.
    ///
    /// If `show_fail` passed to [`Tracker::new`] is `true`, error messages will be
    /// printed when tasks fail.
    fn wait(mut self, rx: Receiver<Report<R>>) -> Vec<Result<R>> {
        let start = Instant::now();
        while self.done.len() < self.total {
            match rx.recv().unwrap() {
                Report::Running(idx, name) => self.trace_running(idx, name),
                Report::Done(idx, result) => self.trace_done(idx, result),
            }
        }
        let end = Instant::now();
        let elapsed_time = end - start;

        let result = if self.fail_count > 0 {
            style("failed").red().to_string()
        } else {
            style("ok").green().to_string()
        };

        term::cursor_up();
        stderr!();
        stderr!(
            "{} result: {}. {} ok; {} failed; finished in {}",
            self.desc_pure,
            result,
            self.ok_count,
            self.fail_count,
            Self::format_elapsed(elapsed_time),
        );
        if let Some(fail_message) = self.fail_message.as_ref() {
            stderr!();
            stderr!("Error message:");
            for (name, msg) in fail_message {
                stderr!("  {}: {}", name, msg);
            }
            stderr!();
        }

        self.done
            .sort_unstable_by(|(idx1, _), (idx2, _)| idx1.cmp(idx2));

        let results: Vec<_> = self.done.into_iter().map(|(_, result)| result).collect();

        results
    }

    /// Print running task on terminal.
    fn trace_running(&mut self, idx: usize, name: String) {
        self.running.push((idx, name));
        let line = self.render();
        term::cursor_up();
        stderr!("{}", line);
    }

    /// Print completed task on terminal.
    fn trace_done(&mut self, idx: usize, result: Result<R>) {
        let name = match self
            .running
            .iter()
            .position(|(found_idx, _)| found_idx == &idx)
        {
            Some(idx) => {
                let (_, name) = self.running.remove(idx);
                name
            }
            None => return,
        };

        term::cursor_up();
        match result.as_ref() {
            Ok(_) => {
                self.ok_count += 1;
                stderr!("{} {} {}", self.desc_head, name, style("ok").green());
            }
            Err(err) => {
                self.fail_count += 1;
                stderr!("{} {} {}", self.desc_head, name, style("fail").red());
                if self.show_fail {
                    let item = (name, format!("{}", err));
                    match self.fail_message.as_mut() {
                        Some(msg) => msg.push(item),
                        None => self.fail_message = Some(vec![item]),
                    }
                }
            }
        }
        self.done.push((idx, result));
        let line = self.render();
        stderr!("{}", line);
    }

    /// Render tracing line. The format is:
    ///
    /// * `{desc} {progress_bar} ({current_num}/{total_num}) {running}...`
    ///
    /// The tracing line will adaptively change based on the current terminal width.
    /// If the terminal width is too small, some information at the end will be
    /// replaced with ellipsis.
    fn render(&self) -> String {
        let term_size = term::size();
        if self.desc_size > term_size {
            // The terminal is too small, no space to print info, just print "....".
            return ".".repeat(term_size);
        }

        // Render desc (with color).
        let mut line = self.desc.clone();
        if self.desc_size + Self::SPACE_SIZE > term_size || term::bar_size() == 0 {
            return line;
        }
        line.push_str(Self::SPACE);

        // Render progress bar.
        let bar = term::render_bar(self.done.len(), self.total);
        let bar_size = Self::get_size(&bar);
        if Self::get_size(&line) + bar_size > term_size {
            return line;
        }
        line.push_str(&bar);

        if Self::get_size(&line) + Self::SPACE_SIZE > term_size {
            return line;
        }
        line.push_str(Self::SPACE);

        // Render tag.
        let tag = self.render_tag();
        let tag_size = Self::get_size(&bar);
        if Self::get_size(&line) + tag_size > term_size {
            return line;
        }
        line.push_str(&tag);

        if Self::get_size(&line) + Self::SPACE_SIZE > term_size {
            return line;
        }
        line.push_str(Self::SPACE);

        let left = term_size - Self::get_size(&line);
        if left == 0 {
            return line;
        }

        // Render running items
        let list = self.render_list(left);
        line.push_str(&list);
        line
    }

    /// The tracing tag, `({current}/{total})`.
    fn render_tag(&self) -> String {
        let pad = self.total_pad;
        let current = format!("{:pad$}", self.done.len(), pad = pad);
        format!("({current}/{})", self.total)
    }

    /// The running items, `item0, item1, ...`.
    fn render_list(&self, size: usize) -> String {
        let mut list = String::with_capacity(size);
        for (idx, (_, name)) in self.running.iter().enumerate() {
            let add_size = if idx == 0 {
                Self::get_size(&name)
            } else {
                Self::get_size(&name) + Self::SEP_SIZE
            };
            let is_last = idx == self.running.len() - 1;
            let list_size = Self::get_size(&list);
            let new_size = list_size + add_size;
            if new_size > size || (!is_last && new_size == size) {
                let delta = size - list_size;
                if delta == 0 {
                    break;
                }
                if delta < Self::OMIT_SIZE {
                    list.push_str(&".".repeat(delta));
                } else {
                    list.push_str(&Self::OMIT);
                }
                break;
            }
            if idx != 0 {
                list.push_str(Self::SEP);
            }
            list.push_str(&name);
        }
        list
    }

    /// See: [`console::measure_text_width`].
    fn get_size(s: impl AsRef<str>) -> usize {
        console::measure_text_width(s.as_ref())
    }

    /// Show elapsed time.
    fn format_elapsed(d: Duration) -> String {
        let elapsed_time = d.as_secs_f64();

        if elapsed_time >= 3600.0 {
            let hours = elapsed_time / 3600.0;
            format!("{:.2}h", hours)
        } else if elapsed_time >= 60.0 {
            let minutes = elapsed_time / 60.0;
            format!("{:.2}min", minutes)
        } else if elapsed_time >= 1.0 {
            format!("{:.2}s", elapsed_time)
        } else {
            let milliseconds = elapsed_time * 1000.0;
            format!("{:.2}ms", milliseconds)
        }
    }
}

/// Similar to [`run`], but if any task encounters an error during execution, the
/// entire function returns an error.
pub fn must_run<T, R>(desc: &str, tasks: Vec<(String, T)>) -> Result<Vec<R>>
where
    R: Send + 'static,
    T: Task<R> + Send + 'static,
{
    let results = run(desc, tasks, true);
    if !is_ok(&results) {
        bail!("{desc} failed");
    }
    Ok(results.into_iter().map(|r| r.unwrap()).collect())
}

/// Concurrently execute multiple tasks, block the current main thread until all
/// tasks are completed or an error occurs. Return the execution results of these
/// tasks.
///
/// We will start working threads equal to the number of CPU cores on the current
/// machine to execute tasks.
///
/// For how to define the execution function for tasks, see: [`Task`].
///
/// # Arguments
///
/// * `desc` - A descriptive string for the task, which will be printed in the terminal.
/// * `tasks` - The tasks list to execute.
/// * `show_fail` - If `true`, show error messages for tasks after they fail.
pub fn run<T, R>(desc: &str, tasks: Vec<(String, T)>, show_fail: bool) -> Vec<Result<R>>
where
    R: Send + 'static,
    T: Task<R> + Send + 'static,
{
    let task_len = tasks.len();
    if task_len == 0 {
        return vec![];
    }

    // Set the number of workers to the number of cpu cores to maximize the use of
    // multi-core cpu.
    // Here num_cpus can guarantee that the number of cores returned is greater
    // than 0.
    let worker_len = num_cpus::get();
    assert_ne!(worker_len, 0);

    let (task_tx, task_rx) = mpsc::channel::<(usize, String, T)>();
    // By default, mpsc does not support multiple threads using the same consumer.
    // But our multiple workers need to preempt the task queue. Therefore, Arc+Mutex
    // is used here to allow the consumer of mpsc to be preempted by multiple threads
    // at the same time.
    let task_shared_rx = Arc::new(Mutex::new(task_rx));

    // This mpsc channel is used to receive worker progress reports. Including task
    // execution progress and completion status.
    let (report_tx, report_rx) = mpsc::channel::<Report<R>>();

    let title = style(format!("{} with {} workers:", desc, worker_len))
        .bold()
        .cyan()
        .underlined();
    stderr!("{}\n", title);
    let mut handlers = Vec::with_capacity(worker_len);
    for _ in 0..worker_len {
        let task_shared_rx = Arc::clone(&task_shared_rx);
        let report_tx = report_tx.clone();
        let handler = thread::spawn(move || loop {
            // Try to preempt the shared mpsc consumer. If there are other threads
            // occupying the consumer at this time, it will be blocked here.
            let task_rx = match task_shared_rx.lock() {
                Ok(rx) => rx,
                Err(_) => return,
            };

            // Consume a task from shared task channel.
            let recv = task_rx.recv();
            // After acquiring a task, the consumer is released immediately so that
            // other workers can preempt other tasks. Otherwise, while the worker is
            // processing this task, other workers will be blocked when preempting
            // the task, cause tasks cannot be processed asynchronously.
            drop(task_rx);

            if let Ok((idx, name, task)) = recv {
                report_tx.send(Report::Running(idx, name)).unwrap();
                // Running message reporting will be done by task itself.
                let result = task.run();
                report_tx.send(Report::Done(idx, result)).unwrap();
            } else {
                // task_rx is closed, it means that all tasks have been processed.
                // The worker can exit now.
                return;
            }
        });
        handlers.push(handler);
    }

    // Produce all tasks to workers.
    for (idx, (name, task)) in tasks.into_iter().enumerate() {
        task_tx.send((idx, name, task)).unwrap();
    }
    drop(task_tx);

    let tracker = Tracker::new(desc, task_len, show_fail);
    let results = tracker.wait(report_rx);

    // Wait for all workers done.
    for handler in handlers {
        handler.join().unwrap();
    }

    results
}

/// Returns `true` if all tasks are ok.
pub fn is_ok<R>(results: &Vec<Result<R>>) -> bool {
    for result in results {
        if let Err(_) = result {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod batch_tests {
    use std::time::Duration;

    use crate::batch::*;

    struct TestTask {
        idx: usize,
    }

    impl Task<usize> for TestTask {
        fn run(&self) -> Result<usize> {
            thread::sleep(Duration::from_millis(100));
            Ok(self.idx)
        }
    }

    #[test]
    fn test_run() {
        const COUNT: usize = 30;
        let mut tasks = Vec::with_capacity(COUNT);
        for i in 0..COUNT {
            let task = TestTask { idx: i };
            tasks.push((format!("Task-{i}"), task));
        }

        let results: Vec<usize> = run("Test", tasks, false)
            .into_iter()
            .map(|result| result.unwrap())
            .collect();
        assert_eq!(results.len(), COUNT);
        for i in 0..COUNT {
            assert_eq!(i, results[i]);
        }
    }
}

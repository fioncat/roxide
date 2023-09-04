use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{bail, Result};
use console::style;

use crate::shell;

pub trait Task<R: Send> {
    fn run(&self) -> Result<R>;
}

enum Report<R> {
    Running(usize, String),
    Done(usize, Result<R>),
}

struct Tracker<R> {
    total: usize,
    total_pad: usize,
    running: Vec<(usize, String)>,

    done: Vec<(usize, Result<R>)>,

    desc: String,
    desc_pure: String,
    desc_size: usize,
    desc_head: String,

    ok_count: usize,
    fail_count: usize,

    show_fail: bool,
    fail_message: Option<Vec<(String, String)>>,
}

impl<R> Tracker<R> {
    const SPACE: &str = " ";
    const SEP: &str = ", ";
    const OMIT: &str = ", ...";

    const SPACE_SIZE: usize = Self::SPACE.len();
    const SEP_SIZE: usize = Self::SEP.len();
    const OMIT_SIZE: usize = Self::OMIT.len();

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

        shell::cursor_up_stdout();
        println!();
        println!(
            "{} result: {}. {} ok; {} failed; finished in {}",
            self.desc_pure,
            result,
            self.ok_count,
            self.fail_count,
            Self::format_elapsed(elapsed_time),
        );
        if let Some(fail_message) = self.fail_message.as_ref() {
            println!();
            println!("Error message:");
            for (name, msg) in fail_message {
                println!("  {name}: {msg}");
            }
            println!();
        }

        self.done
            .sort_unstable_by(|(idx1, _), (idx2, _)| idx1.cmp(idx2));

        let results: Vec<_> = self.done.into_iter().map(|(_, result)| result).collect();

        results
    }

    fn trace_running(&mut self, idx: usize, name: String) {
        self.running.push((idx, name));
        let line = self.render();
        shell::cursor_up_stdout();
        println!("{line}");
    }

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

        shell::cursor_up_stdout();
        match result.as_ref() {
            Ok(_) => {
                self.ok_count += 1;
                println!("{} {name} {}", self.desc_head, style("ok").green());
            }
            Err(err) => {
                self.fail_count += 1;
                println!("{} {name} {}", self.desc_head, style("fail").red());
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
        println!("{line}");
    }

    fn render(&self) -> String {
        let term_size = shell::size();
        if self.desc_size > term_size {
            return ".".repeat(term_size);
        }

        let mut line = self.desc.clone();
        if self.desc_size + Self::SPACE_SIZE > term_size || shell::bar_size() == 0 {
            return line;
        }
        line.push_str(Self::SPACE);

        let bar = shell::render_bar(self.done.len(), self.total);
        let bar_size = Self::get_size(&bar);
        if Self::get_size(&line) + bar_size > term_size {
            return line;
        }
        line.push_str(&bar);

        if Self::get_size(&line) + Self::SPACE_SIZE > term_size {
            return line;
        }
        line.push_str(Self::SPACE);

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

        let list = self.render_list(left);
        line.push_str(&list);
        line
    }

    fn render_tag(&self) -> String {
        let pad = self.total_pad;
        let current = format!("{:pad$}", self.done.len(), pad = pad);
        format!("({current}/{})", self.total)
    }

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

    fn get_size(s: impl AsRef<str>) -> usize {
        console::measure_text_width(s.as_ref())
    }

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
    println!("{title}\n");
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

pub fn is_ok<R>(results: &Vec<Result<R>>) -> bool {
    for result in results {
        if let Err(_) = result {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use serial_test::serial;

    use super::*;

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
    #[serial]
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

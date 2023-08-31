use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;

use anyhow::Result;
use console::{style, Term};

pub trait Task<R: Send> {
    fn name(&self) -> String;

    fn run(&self) -> Result<R>;
}

enum Report<R> {
    Running(usize, String),
    Done(usize, Result<R>, String),
}

struct Tracker<R> {
    total: usize,
    total_pad: usize,
    running: Vec<(usize, String)>,

    done: Vec<(usize, Result<R>)>,

    desc: String,
    desc_size: usize,
    desc_head: String,

    term_size: usize,
    bar_size: usize,

    ok_count: usize,
    fail_count: usize,
}

impl<R> Tracker<R> {
    const SPACE: &str = " ";
    const SEP: &str = ", ";
    const OMIT: &str = ", ...";

    const SPACE_SIZE: usize = 1;
    const SEP_SIZE: usize = 2;
    const OMIT_SIZE: usize = 5;

    fn new(desc: &str, total: usize) -> Tracker<R> {
        let desc = style(desc).green().bold().to_string();
        let desc_size = Self::get_size(&desc);

        let total_pad = total.to_string().chars().count();

        let term = Term::stdout();
        let (_, col_size) = term.size();
        let term_size = col_size as usize;

        let bar_size = if term_size <= 20 { 0 } else { term_size / 4 };

        Tracker {
            total,
            total_pad,
            running: Vec::with_capacity(total),
            done: Vec::with_capacity(total),
            desc,
            desc_size,
            desc_head: " ".repeat(desc_size),
            term_size,
            bar_size,
            ok_count: 0,
            fail_count: 0,
        }
    }

    fn wait(mut self, rx: Receiver<Report<R>>) -> Vec<Result<R>> {
        println!();
        while self.done.len() < self.total {
            match rx.recv().unwrap() {
                Report::Running(idx, name) => self.trace_running(idx, name),
                Report::Done(idx, result, name) => self.trace_done(idx, name, result),
            }
        }
        Self::cursor_up();
        let result = if self.fail_count > 0 {
            style("failed").red().to_string()
        } else {
            style("ok").green().to_string()
        };
        println!(
            "{} result: {}. {} ok; {} failed",
            self.desc, result, self.ok_count, self.fail_count,
        );
        self.done
            .sort_unstable_by(|(idx1, _), (idx2, _)| idx1.cmp(idx2));
        self.done.into_iter().map(|(_, result)| result).collect()
    }

    fn trace_running(&mut self, idx: usize, name: String) {
        self.running.push((idx, name));
        let line = self.render();
        Self::cursor_up();
        println!("{line}");
    }

    fn trace_done(&mut self, idx: usize, name: String, result: Result<R>) {
        if let Some(pos) = self
            .running
            .iter()
            .position(|(found_idx, _)| found_idx == &idx)
        {
            self.running.remove(pos);
        }

        Self::cursor_up();
        match result.as_ref() {
            Ok(_) => {
                self.ok_count += 1;
                println!("{} {name} {}", self.desc_head, style("ok").green());
            }
            Err(err) => {
                self.fail_count += 1;
                println!("{} {name} {}: {err}", self.desc_head, style("fail").red());
            }
        }
        self.done.push((idx, result));
        let line = self.render();
        println!("{line}");
    }

    fn render(&self) -> String {
        if self.desc_size > self.term_size {
            return ".".repeat(self.term_size);
        }

        let mut line = self.desc.clone();
        if self.desc_size + Self::SPACE_SIZE > self.term_size || self.bar_size == 0 {
            return line;
        }
        line.push_str(Self::SPACE);

        let bar = self.render_bar();
        let bar_size = Self::get_size(&bar);
        if Self::get_size(&line) + bar_size > self.term_size {
            return line;
        }
        line.push_str(&bar);

        if Self::get_size(&line) + Self::SPACE_SIZE > self.term_size {
            return line;
        }
        line.push_str(Self::SPACE);

        let tag = self.render_tag();
        let tag_size = Self::get_size(&bar);
        if Self::get_size(&line) + tag_size > self.term_size {
            return line;
        }
        line.push_str(&tag);

        if Self::get_size(&line) + Self::SPACE_SIZE > self.term_size {
            return line;
        }
        line.push_str(Self::SPACE);

        let left = self.term_size - Self::get_size(&line);
        if left == 0 {
            return line;
        }

        let list = self.render_list(left);
        line.push_str(&list);
        line
    }

    fn render_bar(&self) -> String {
        let current_count = if self.done.len() >= self.total {
            self.bar_size
        } else {
            let percent = (self.done.len() as f64) / (self.total as f64);
            let current_f64 = (self.bar_size as f64) * percent;
            let current = current_f64 as u64 as usize;
            if current >= self.bar_size {
                self.bar_size
            } else {
                current
            }
        };
        let current = "#".repeat(current_count);
        if current_count >= self.bar_size {
            return format!("[{current}]");
        }

        let pending = " ".repeat(self.bar_size - current_count);
        format!("[{current}{pending}]")
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
            let list_size = Self::get_size(&list);
            if list_size + add_size > size {
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

    fn cursor_up() {
        print!("\x1b[A");
        print!("\x1b[K");
    }
}

pub fn run<T, R>(desc: &str, tasks: Vec<T>) -> Vec<Result<R>>
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

    let (task_tx, task_rx) = mpsc::channel::<(usize, T)>();
    // By default, mpsc does not support multiple threads using the same consumer.
    // But our multiple workers need to preempt the task queue. Therefore, Arc+Mutex
    // is used here to allow the consumer of mpsc to be preempted by multiple threads
    // at the same time.
    let task_shared_rx = Arc::new(Mutex::new(task_rx));

    // This mpsc channel is used to receive worker progress reports. Including task
    // execution progress and completion status.
    let (report_tx, report_rx) = mpsc::channel::<Report<R>>();

    println!(
        "{} Starting {} workers",
        style("==>").green().bold(),
        worker_len
    );
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

            if let Ok((idx, task)) = recv {
                let name = task.name();
                report_tx.send(Report::Running(idx, name.clone())).unwrap();
                // Running message reporting will be done by task itself.
                let result = task.run();
                report_tx.send(Report::Done(idx, result, name)).unwrap();
            } else {
                // task_rx is closed, it means that all tasks have been processed.
                // The worker can exit now.
                return;
            }
        });
        handlers.push(handler);
    }

    // Produce all tasks to workers.
    for (idx, task) in tasks.into_iter().enumerate() {
        task_tx.send((idx, task)).unwrap();
    }
    drop(task_tx);

    let tracker = Tracker::new(desc, task_len);
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
        fn name(&self) -> String {
            format!("Task {} done!", self.idx)
        }

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
            tasks.push(task);
        }

        let results: Vec<usize> = run("Test", tasks)
            .into_iter()
            .map(|result| result.unwrap())
            .collect();
        assert_eq!(results.len(), COUNT);
        for i in 0..COUNT {
            assert_eq!(i, results[i]);
        }
    }
}

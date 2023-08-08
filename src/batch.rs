use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

use anyhow::Result;
use console::style;

use crate::exec;

pub trait Task<R: Send> {
    fn message_done(&self, result: &Result<R>) -> String;

    fn run(&self, rp: &Reporter<R>) -> Result<R>;
}

enum Report<R> {
    Running(usize, String),
    Done(usize, Result<R>, String),
}

pub struct Reporter<'a, R> {
    idx: usize,
    report_tx: &'a Sender<Report<R>>,
}

impl<'a, R> Reporter<'_, R> {
    pub fn message(&self, msg: String) {
        self.report_tx.send(Report::Running(self.idx, msg)).unwrap();
    }

    fn done(&self, result: Result<R>, msg: String) {
        self.report_tx
            .send(Report::Done(self.idx, result, msg))
            .unwrap();
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

    exec!("{}: Starting {} workers", desc, worker_len);
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
                let rp = Reporter {
                    idx,
                    report_tx: &report_tx,
                };
                // Running message reporting will be done by task itself.
                let result = task.run(&rp);
                let msg = task.message_done(&result);
                rp.done(result, msg);
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

    let mut suc_count = 0;
    let mut fail_count = 0;
    let mut running: Vec<(usize, String)> = Vec::with_capacity(task_len);
    let mut results: Vec<(usize, Result<R>)> = Vec::with_capacity(task_len);
    while results.len() < task_len {
        match report_rx.recv().unwrap() {
            Report::Running(idx, msg) => {
                if let Some(_) = results.iter().position(|(task_idx, _)| *task_idx == idx) {
                    // If the task is already done, we won't show its running
                    // message.
                    continue;
                }
                // Update or push.
                if let Some(running_idx) = running.iter().position(|(task_idx, _)| *task_idx == idx)
                {
                    // Update running message.
                    running[running_idx] = (idx, msg);
                    cursor_up(running.len());
                } else {
                    // Add new running task.
                    cursor_up(running.len());
                    running.push((idx, msg));
                }
                show_runnings(&running);
            }
            Report::Done(idx, result, msg) => {
                cursor_up(running.len());
                let ok = match result {
                    Ok(_) => {
                        suc_count += 1;
                        true
                    }
                    Err(_) => {
                        fail_count += 1;
                        false
                    }
                };
                show_done(ok, msg, results.len(), task_len);
                if let Some(running_idx) = running.iter().position(|(task_idx, _)| *task_idx == idx)
                {
                    running.remove(running_idx);
                }
                results.push((idx, result));
                show_runnings(&running);
            }
        }
    }
    if !running.is_empty() {
        cursor_up(running.len());
    }

    // Wait for all workers done.
    for handler in handlers {
        handler.join().unwrap();
    }

    println!(
        "{desc} done, with {} successed, {} failed",
        style(suc_count).green(),
        style(fail_count).red()
    );
    results.sort_unstable_by(|(idx1, _), (idx2, _)| idx1.cmp(idx2));
    results.into_iter().map(|(_, result)| result).collect()
}

fn show_done(ok: bool, msg: String, current: usize, total: usize) {
    let pad_len = total.to_string().chars().count();
    let current_pad = format!("{:pad_len$}", current + 1, pad_len = pad_len);
    let prefix = format!("({current_pad}/{total})");
    if ok {
        println!("{} {} {msg}", style(prefix).bold(), style("==>").green());
    } else {
        println!("{} {} {msg}", style(prefix).bold(), style("==>").red());
    }
}

fn show_running(msg: &String) {
    println!("{} {msg}", style("==>").cyan());
}

fn show_runnings(running: &Vec<(usize, String)>) {
    for (_, msg) in running {
        show_running(&msg);
    }
}

// UNIX only, terminal move cursor up for n line(s).
fn cursor_up(n: usize) {
    if n == 0 {
        return;
    }
    for _ in 0..n {
        print!("\x1b[A");
        print!("\x1b[K");
    }
}

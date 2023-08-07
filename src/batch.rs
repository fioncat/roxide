use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;

use anyhow::Result;
use console::style;

pub trait Task<R: Send> {
    fn message_running(&self) -> String;
    fn message_done(&self, result: &Result<R>) -> String;

    fn run(&self) -> Result<R>;
}

enum Report<R> {
    Running(usize, String),
    Done(usize, Result<R>, String),
}

pub fn run<T, R>(desc: &str, tasks: Vec<T>) -> Vec<Result<R>>
where
    R: Send + 'static,
    T: Task<R> + Send + 'static,
{
    let task_len = tasks.len();
    let mut results = Vec::with_capacity(task_len);
    if task_len == 0 {
        return results;
    }

    // In order to ensure the maximum utilization of the cpu, the number of started
    // threads is equal to the number of cpus. If the cpu is single-core, directly
    // process and return.
    let worker_len = num_cpus::get();
    assert_ne!(worker_len, 0);

    if worker_len == 1 || task_len == 1 {
        println!("Starting 1 worker");
        // No need to spawn threads, handle myself.
        for (idx, task) in tasks.iter().enumerate() {
            let msg = task.message_running();
            show_running(&msg);
            let result = task.run();
            let msg = task.message_done(&result);
            let ok = match &result {
                Ok(_) => true,
                Err(_) => false,
            };
            show_done(ok, msg, idx, task_len);
            results.push(result);
        }

        return results;
    }

    let (task_tx, task_rx) = mpsc::channel::<(usize, T)>();
    // The rx of mpsc is not shareable, we need to convert it to shareable.
    // And need to use Mutex to protect it.
    let task_shared_rx = Arc::new(Mutex::new(task_rx));

    // This is used to recv handle results returned by workers.
    let (report_tx, report_rx) = mpsc::channel::<Report<R>>();

    println!("Starting {} workers", worker_len);
    let mut handlers = Vec::with_capacity(worker_len);
    for _ in 0..worker_len {
        let task_shared_rx = Arc::clone(&task_shared_rx);
        let report_tx = report_tx.clone();
        let handler = thread::spawn(move || loop {
            let task_rx = match task_shared_rx.lock() {
                Ok(rx) => rx,
                Err(_) => return,
            };

            // Consume a task from shared task channel.
            let recv = task_rx.recv();
            // Dropping task_rx here can release the Mutex that protects it at the
            // same time, so that other workers can obtain tasks from task_shared_rx
            // asynchronously.
            drop(task_rx);

            if let Ok((idx, task)) = recv {
                // The result_tx will be closed only after all workers done, so it
                // is safe to call `unwrap` here.
                let msg = task.message_running();
                report_tx.send(Report::Running(idx, msg)).unwrap();
                let result = task.run();
                let msg = task.message_done(&result);
                report_tx.send(Report::Done(idx, result, msg)).unwrap();
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
                    continue;
                }
                cursor_up(running.len());
                running.push((idx, msg));
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

fn cursor_up(n: usize) {
    if n == 0 {
        return;
    }
    for _ in 0..n {
        print!("\x1b[A");
        print!("\x1b[K");
    }
}

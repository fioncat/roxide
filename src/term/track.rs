use std::sync::Arc;

use anyhow::{Result, bail};
use console::style;
use tokio::sync::mpsc::Receiver;
use tokio::time::Instant;

use crate::format::format_elapsed;
use crate::{cursor_up, debug, outputln, term};

/// Used by [`Tracker`] for reporting the execution results of tasks from worker
/// threads to the main thread.
pub enum Report<R> {
    /// Indicates that the task is still in progress, where `usize` is the current
    /// task index and `String` is the name of the current task.
    Running(usize, Arc<String>),

    /// Indicates that the task has been completed. `usize` is the current task
    /// index, and `Result<R>` represents the execution result of the task.
    Done(usize, Result<R>),
}

/// `Tracker` is used to track the execution status of batch tasks and output the
/// status to the terminal in real-time.
pub struct Tracker<R> {
    /// total tasks length.
    total: usize,
    /// padding size for `total`.
    total_pad: usize,

    /// running tasks.
    running: Vec<(usize, Arc<String>)>,
    /// done tasks.
    done: Vec<(usize, Result<R>)>,

    verb: String,
    verb_size: usize,
    verb_head: String,
    desc: String,

    ok_count: usize,
    fail_count: usize,

    fail_message: Vec<(Arc<String>, String)>,
}

impl<R> Tracker<R> {
    const SPACE: &'static str = " ";
    const SPACE_SIZE: usize = Self::SPACE.len();

    /// Create a Tracker, call [`Tracker::wait`] later to start tracking.
    pub fn new(verb: &str, desc: &str, total: usize) -> Tracker<R> {
        let verb = style(verb).cyan().bold().to_string();
        let verb_size = console::measure_text_width(&verb);

        let total_pad = total.to_string().chars().count();

        Tracker {
            total,
            total_pad,
            running: Vec::with_capacity(total),
            done: Vec::with_capacity(total),
            verb,
            verb_size,
            verb_head: " ".repeat(verb_size),
            desc: desc.to_string(),
            ok_count: 0,
            fail_count: 0,
            fail_message: vec![],
        }
    }

    /// Receive the execution results of tasks from `rx` and track them in real-time.
    /// This function will block the main thread until the expected number of
    /// completed tasks is reached and return the task execution results.
    pub async fn wait(mut self, mut rx: Receiver<Report<R>>) -> Result<Vec<R>> {
        debug!("[tracker] Waiting for {} {} tasks", self.total, self.desc);
        let start = Instant::now();
        while self.done.len() < self.total {
            match rx.recv().await.unwrap() {
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

        cursor_up!();
        outputln!();
        outputln!(
            "{} result: {result}. {} ok; {} failed; finished in {}",
            self.desc,
            self.ok_count,
            self.fail_count,
            format_elapsed(elapsed_time),
        );
        debug!(
            "[tracker] All {} tasks done, ok: {}, fail: {}",
            self.desc, self.ok_count, self.fail_count
        );
        if !self.fail_message.is_empty() {
            outputln!();
            outputln!("Error message:");
            for (name, msg) in self.fail_message {
                outputln!("  {name}: {msg}");
            }
            outputln!();
            bail!("{} task failed", self.desc);
        }

        self.done
            .sort_unstable_by(|(idx1, _), (idx2, _)| idx1.cmp(idx2));

        let results: Vec<_> = self
            .done
            .into_iter()
            .map(|(_, result)| result.unwrap())
            .collect();
        Ok(results)
    }

    /// Print running task on terminal.
    fn trace_running(&mut self, idx: usize, name: Arc<String>) {
        debug!("[tracker] Tracing running {idx}: {name}");
        self.running.push((idx, name));
        let line = self.render();
        cursor_up!();
        outputln!("{}", line);
        debug!(
            "[tracker] Tracing running {idx} finished, running: {}",
            self.running.len()
        );
    }

    /// Print completed task on terminal.
    fn trace_done(&mut self, idx: usize, result: Result<R>) {
        debug!("[tracker] Trace done: {idx}, is_ok: {}", result.is_ok());
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

        cursor_up!();
        match result.as_ref() {
            Ok(_) => {
                self.ok_count += 1;
                outputln!("{} {} {}", self.verb_head, name, style("ok").green());
            }
            Err(err) => {
                self.fail_count += 1;
                outputln!("{} {} {}", self.verb_head, name, style("fail").red());
                self.fail_message.push((name, format!("{err}")));
            }
        }
        self.done.push((idx, result));
        let line = self.render();
        outputln!("{}", line);
        debug!(
            "[tracker] Trace done {idx} finished, count: {}, ok: {}, fail: {}",
            self.done.len(),
            self.ok_count,
            self.fail_count
        );
    }

    /// Render tracing line. The format is:
    ///
    /// * `{desc} {progress_bar} ({current_num}/{total_num}) {running}...`
    ///
    /// The tracing line will adaptively change based on the current terminal width.
    /// If the terminal width is too small, some information at the end will be
    /// replaced with ellipsis.
    fn render(&self) -> String {
        let term_size = super::width();
        if self.verb_size > term_size {
            // The terminal is too small, no space to print info, just print "....".
            return ".".repeat(term_size);
        }

        // Render desc (with color).
        let mut line = self.verb.clone();
        if self.verb_size + Self::SPACE_SIZE > term_size || Self::bar_size() == 0 {
            return line;
        }
        line.push_str(Self::SPACE);

        // Render progress bar.
        let bar = Self::render_bar(self.done.len(), self.total);
        let bar_size = console::measure_text_width(&bar);
        if console::measure_text_width(&line) + bar_size > term_size {
            return line;
        }
        line.push_str(&bar);

        if console::measure_text_width(&line) + Self::SPACE_SIZE > term_size {
            return line;
        }
        line.push_str(Self::SPACE);

        // Render tag.
        let tag = self.render_tag();
        let tag_size = console::measure_text_width(&bar);
        if console::measure_text_width(&line) + tag_size > term_size {
            return line;
        }
        line.push_str(&tag);

        if console::measure_text_width(&line) + Self::SPACE_SIZE > term_size {
            return line;
        }
        line.push_str(Self::SPACE);

        let left = term_size - console::measure_text_width(&line);
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
        let list = self
            .running
            .iter()
            .map(|(_, name)| name.as_str())
            .collect::<Vec<_>>();
        let count = list.len();
        term::render_list(list, count, size)
    }

    /// Return the progress bar size for current terminal.
    fn bar_size() -> usize {
        let term_size = super::width();
        if term_size <= 20 { 0 } else { term_size / 4 }
    }

    /// Render the progress bar.
    pub fn render_bar(current: usize, total: usize) -> String {
        let bar_size = Self::bar_size();
        let current_count = if current >= total {
            bar_size
        } else {
            let percent = (current as f64) / (total as f64);
            let current_f64 = (bar_size as f64) * percent;
            let current = current_f64 as u64 as usize;
            if current >= bar_size {
                bar_size
            } else {
                current
            }
        };
        let current = match current_count {
            0 => String::new(),
            1 => String::from(">"),
            _ => format!("{}>", "=".repeat(current_count - 1)),
        };
        if current_count >= bar_size {
            return format!("[{current}]");
        }

        let pending = " ".repeat(bar_size - current_count);
        format!("[{current}{pending}]")
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::sync::mpsc;
    use tokio::time;

    use super::*;

    #[tokio::test]
    async fn test_tracker() {
        let count = 20;

        let tracker: Tracker<String> = Tracker::new("Testing", "Test", count);
        let (tx, rx) = mpsc::channel::<Report<String>>(10);

        for i in 0..count {
            let tx = tx.clone();
            tokio::spawn(async move {
                tx.send(Report::Running(i, Arc::new(format!("task-{i}"))))
                    .await
                    .unwrap();
                time::sleep(Duration::from_millis(500)).await;
                tx.send(Report::Done(i, Ok(format!("result-{i}"))))
                    .await
                    .unwrap();
            });
        }

        let results = tracker.wait(rx).await.unwrap();
        assert_eq!(results.len(), count);
        for (i, result) in results.iter().enumerate() {
            assert_eq!(result, &format!("result-{i}"));
        }
    }

    #[tokio::test]
    async fn test_tracker_fail() {
        let count = 20;

        let tracker: Tracker<String> = Tracker::new("Testing", "Test", count);
        let (tx, rx) = mpsc::channel::<Report<String>>(10);

        for i in 0..count {
            let tx = tx.clone();
            tokio::spawn(async move {
                tx.send(Report::Running(i, Arc::new(format!("task-{i}"))))
                    .await
                    .unwrap();
                time::sleep(Duration::from_millis(500)).await;
                if i % 2 == 0 {
                    tx.send(Report::Done(i, Ok(format!("result-{i}"))))
                        .await
                        .unwrap();
                } else {
                    tx.send(Report::Done(i, Err(anyhow::anyhow!("error-{i}"))))
                        .await
                        .unwrap();
                }
            });
        }

        let result = tracker.wait(rx).await;
        assert_eq!(result.unwrap_err().to_string(), "Test task failed");
    }
}

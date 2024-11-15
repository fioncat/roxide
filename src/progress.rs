use std::fs::OpenOptions;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use console::style;
use reqwest::blocking::Client;
use reqwest::{Method, Url};

use crate::info;
use crate::term;
use crate::utils;

struct ProgressWrapper {
    desc: String,
    done_desc: String,
    desc_size: usize,

    last_report: Instant,

    current: usize,
    total: usize,

    start: Instant,

    done: bool,
}

impl ProgressWrapper {
    const SPACE: &'static str = " ";
    const SPACE_SIZE: usize = 1;

    const REPORT_INTERVAL: Duration = Duration::from_millis(200);

    pub fn new(desc: String, done_desc: String, total: usize) -> ProgressWrapper {
        let desc_size = console::measure_text_width(&desc);
        let last_report = Instant::now();

        let pw = ProgressWrapper {
            desc,
            done_desc,
            desc_size,
            last_report,
            current: 0,
            total,
            start: Instant::now(),
            done: false,
        };
        eprintln!("{}", pw.render());
        pw
    }

    fn render(&self) -> String {
        let term_size = term::size();
        if self.desc_size > term_size {
            return ".".repeat(term_size);
        }

        let mut line = self.desc.clone();
        if self.desc_size + Self::SPACE_SIZE > term_size || term::bar_size() == 0 {
            return line;
        }
        line.push_str(Self::SPACE);

        let bar = term::render_bar(self.current, self.total);
        let bar_size = console::measure_text_width(&bar);
        let line_size = console::measure_text_width(&line);
        if line_size + bar_size > term_size {
            return line;
        }
        line.push_str(&bar);

        let line_size = console::measure_text_width(&line);
        if line_size + Self::SPACE_SIZE > term_size {
            return line;
        }
        line.push_str(Self::SPACE);

        let info = utils::human_bytes(self.current as u64);
        let info_size = console::measure_text_width(&info);
        let line_size = console::measure_text_width(&line);
        if line_size + info_size > term_size {
            return line;
        }
        line.push_str(&info);

        let line_size = console::measure_text_width(&line);
        if line_size + Self::SPACE_SIZE > term_size {
            return line;
        }
        let elapsed_seconds = self.start.elapsed().as_secs_f64();
        if elapsed_seconds == 0.0 {
            return line;
        }

        line.push_str(Self::SPACE);

        let speed = self.current as f64 / elapsed_seconds;
        let speed = format!("- {}/s", utils::human_bytes(speed as u64));
        let speed_size = console::measure_text_width(&speed);
        let line_size = console::measure_text_width(&line);
        if line_size + speed_size > term_size {
            return line;
        }
        line.push_str(&speed);

        line
    }

    fn update_current(&mut self, size: usize) {
        if self.done {
            return;
        }
        self.current += size;

        if self.current >= self.total {
            self.done = true;
            self.current = self.total;
            term::cursor_up();
            info!("{} {}", self.done_desc, style("done").green());
            return;
        }

        let now = Instant::now();
        let delta = now - self.last_report;
        if delta >= Self::REPORT_INTERVAL {
            term::cursor_up();
            eprintln!("{}", self.render());
            self.last_report = now;
        }
    }
}

impl Drop for ProgressWrapper {
    fn drop(&mut self) {
        if self.done || self.current >= self.total {
            return;
        }
        // The progress didn't stop normally, mark it as failed.
        term::cursor_up();
        info!("{} {}", self.done_desc, style("failed").red());
    }
}

struct ProgressWriter<W: Write> {
    upstream: W,
    wrapper: ProgressWrapper,
}

impl<W: Write> ProgressWriter<W> {
    pub fn new(desc: String, done_desc: String, total: usize, upstream: W) -> ProgressWriter<W> {
        ProgressWriter {
            upstream,
            wrapper: ProgressWrapper::new(desc, done_desc, total),
        }
    }
}

impl<W: Write> Write for ProgressWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let size = self.upstream.write(buf)?;
        self.wrapper.update_current(size);

        Ok(size)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.upstream.flush()
    }
}

pub struct ProgressReader<R: Read> {
    upstream: R,
    wrapper: ProgressWrapper,
}

impl<R: Read> ProgressReader<R> {
    pub fn new(desc: String, done_desc: String, total: usize, upstream: R) -> ProgressReader<R> {
        ProgressReader {
            upstream,
            wrapper: ProgressWrapper::new(desc, done_desc, total),
        }
    }
}

impl<R: Read> Read for ProgressReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let size = self.upstream.read(buf)?;
        self.wrapper.update_current(size);

        Ok(size)
    }
}

pub fn download(name: &str, url: impl AsRef<str>, path: impl AsRef<str>) -> Result<()> {
    const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(10);

    let client = Client::builder().timeout(DOWNLOAD_TIMEOUT).build().unwrap();
    let url = Url::parse(url.as_ref()).context("parse download url")?;

    let req = client
        .request(Method::GET, url)
        .build()
        .context("build download http request")?;

    let mut resp = client.execute(req).context("request http download")?;
    let total = match resp.content_length() {
        Some(size) => size,
        None => bail!("could not find content-length in http response"),
    };

    let path = PathBuf::from(path.as_ref());
    utils::ensure_dir(&path)?;

    let file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&path)
        .with_context(|| format!("Open file {}", path.display()))?;
    let desc = format!("Downloading {name}:");

    let mut pw = ProgressWriter::new(desc, "Download".to_string(), total as usize, file);
    resp.copy_to(&mut pw).context("download data")?;

    Ok(())
}

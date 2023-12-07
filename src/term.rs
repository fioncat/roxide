#![allow(dead_code)]

use std::collections::HashSet;
use std::env;
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::rc::Rc;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use chrono::Local;
use console::{style, StyledObject, Term};
use dialoguer::theme::ColorfulTheme;
use dialoguer::Input;
use pad::PadStr;
use regex::{Captures, Regex};
use reqwest::blocking::Client;
use reqwest::{Method, Url};
use serde::Serialize;
use serde_json::ser::PrettyFormatter;
use serde_json::Serializer;

use crate::api::Provider;
use crate::config::{Config, WorkflowStep};
use crate::errors::SilentExit;
use crate::repo::Repo;
use crate::utils;

/// The macro for [`must_confirm`].
///
/// # Examples
///
/// ```
/// conform!("Do you want to delete the repo");
/// conform!("Do you want to delete the repo {}", "hello");
/// ```
#[macro_export]
macro_rules! confirm {
    ($dst:expr $(,)?) => {
        $crate::term::must_confirm($dst)?;
    };
    ($fmt:expr, $($arg:tt)*) => {
        let msg = format!($fmt, $($arg)*);
        $crate::term::must_confirm(msg.as_str())?;
    };
}

/// The macro for [`show_exec`].
///
/// # Examples
///
/// ```
/// exec!("Execute git command");
/// exec!("Execute {} command", "git");
/// ```
#[macro_export]
macro_rules! exec {
    ($dst:expr $(,)?) => {
        {
            $crate::term::show_exec($dst);
        }
    };
    ($fmt:expr, $($arg:tt)*) => {
        {
            let msg = format!($fmt, $($arg)*);
            $crate::term::show_exec(msg.as_str());
        }
    };
}

/// The macro for [`show_info`].
///
/// # Examples
///
/// ```
/// info!("Process repo done");
/// info!("Process repo {} done", "hello");
/// ```
#[macro_export]
macro_rules! info {
    ($dst:expr $(,)?) => {
        {
            $crate::term::show_info($dst);
        }
    };
    ($fmt:expr, $($arg:tt)*) => {
        {
            let msg = format!($fmt, $($arg)*);
            $crate::term::show_info(msg.as_str());
        }
    };
}

/// The macro for [`write_stderr`].
///
/// # Examples
///
/// ```
/// stderr!("Process repo done");
/// stderr!("Process repo {} done", "hello");
/// ```
#[macro_export]
macro_rules! stderr {
    () => {
        {
            $crate::term::write_stderr("");
        }
    };
    ($dst:expr $(,)?) => {
        {
            $crate::term::write_stderr($dst);
        }
    };
    ($fmt:expr, $($arg:tt)*) => {
        {
            let msg = format!($fmt, $($arg)*);
            $crate::term::write_stderr(msg.as_str());
        }
    };
}

/// Display logs at the `exec` level.
pub fn show_exec(msg: impl AsRef<str>) {
    let msg = format!("{} {}", style("==>").cyan(), msg.as_ref());
    write_stderr(msg);
}

/// Display logs at the `info` level.
pub fn show_info(msg: impl AsRef<str>) {
    let msg = format!("{} {}", style("==>").green(), msg.as_ref());
    write_stderr(msg);
}

/// Output the content to `stderr`.
pub fn write_stderr(msg: impl AsRef<str>) {
    if cfg!(test) {
        // In testing, do not print anything
        return;
    }

    _ = writeln!(io::stderr(), "{}", msg.as_ref());
}

/// Output the object in pretty JSON format in the terminal.
pub fn show_json<T: Serialize>(value: T) -> Result<()> {
    let formatter = PrettyFormatter::with_indent(b"  ");
    let mut buf = Vec::new();
    let mut ser = Serializer::with_formatter(&mut buf, formatter);
    value.serialize(&mut ser).context("serialize object")?;
    let json = String::from_utf8(buf).context("encode json utf8")?;
    println!("{json}");
    Ok(())
}

/// Move the cursor up by one line.
pub fn cursor_up() {
    if cfg!(test) {
        return;
    }
    const CURSOR_UP_CHARS: &'static str = "\x1b[A\x1b[K";
    _ = write!(io::stderr(), "{CURSOR_UP_CHARS}");
}

/// Return the current terminal width size.
pub fn size() -> usize {
    let term = Term::stdout();
    let (_, col_size) = term.size();
    col_size as usize
}

/// Return the progress bar size for current terminal.
pub fn bar_size() -> usize {
    let term_size = size();
    if term_size <= 20 {
        0
    } else {
        term_size / 4
    }
}

/// Render the progress bar.
pub fn render_bar(current: usize, total: usize) -> String {
    let bar_size = bar_size();
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

/// Ask user to confirm.
pub fn confirm(msg: impl AsRef<str>) -> Result<bool> {
    if cfg!(test) {
        // In testing, skip confirm.
        return Ok(true);
    }

    let msg = format!(":: {}?", msg.as_ref());
    _ = write!(io::stderr(), "{} [Y/n] ", style(msg).bold());

    let mut answer = String::new();
    scanf::scanf!("{}", answer).context("confirm: scan terminal stdin")?;
    if answer.to_lowercase() != "y" {
        return Ok(false);
    }

    return Ok(true);
}

/// Ask user to confirm. Return error if user choose `no`.
pub fn must_confirm(msg: impl AsRef<str>) -> Result<()> {
    let ok = confirm(msg)?;
    if !ok {
        bail!(SilentExit { code: 60 });
    }
    Ok(())
}

/// Ask user to confirm operation. Display multiple items.
pub fn confirm_items(
    items: &Vec<String>,
    action: &str,
    noun: &str,
    name: &str,
    plural: &str,
) -> Result<bool> {
    if cfg!(test) {
        // In testing, skip confirm.
        return Ok(true);
    }

    if items.is_empty() {
        stderr!("Nothing to {}", action);
        return Ok(false);
    }

    info!("Require confirm to {}", action);
    stderr!();

    let term = Term::stdout();
    let (_, col_size) = term.size();
    let col_size = col_size as usize;

    let name = if items.len() == 1 { name } else { plural };
    let head = format!("{} ({}): ", name, items.len());
    let head_size = console::measure_text_width(head.as_str());
    let head_space = " ".repeat(head_size);

    let mut current_size: usize = 0;
    for (idx, item) in items.iter().enumerate() {
        let item_size = console::measure_text_width(&item);
        if current_size == 0 {
            if idx == 0 {
                _ = write!(io::stderr(), "{head}{item}");
            } else {
                _ = write!(io::stderr(), "{head_space}{item}");
            }
            current_size = head_size + item_size;
            continue;
        }

        current_size += 2 + item_size;
        if current_size > col_size {
            stderr!();
            _ = write!(io::stderr(), "{head_space}{item}");
            current_size = head_size + item_size;
            continue;
        }

        _ = write!(io::stderr(), "  {item}");
    }
    stderr!("\n");

    stderr!(
        "Total {} {} to {}",
        items.len(),
        name.to_lowercase(),
        action
    );
    stderr!();
    let ok = confirm(format!("Proceed with {noun}"))?;
    if ok {
        stderr!();
    }
    Ok(ok)
}

/// The same as [`confirm_items`], return error if user choose `no`.
pub fn must_confirm_items(
    items: &Vec<String>,
    action: &str,
    noun: &str,
    name: &str,
    plural: &str,
) -> Result<()> {
    if !confirm_items(items, action, noun, name, plural)? {
        bail!(SilentExit { code: 60 });
    }
    Ok(())
}

/// Ask user to input string.
pub fn input(msg: impl AsRef<str>, require: bool, default: Option<&str>) -> Result<String> {
    if cfg!(test) {
        return match default {
            Some(s) => Ok(String::from(s)),
            None if require => bail!("test does not support input new string"),
            None => Ok(String::new()),
        };
    }

    let theme = ColorfulTheme::default();
    let mut input: Input<String> = Input::with_theme(&theme);
    input.with_prompt(msg.as_ref());
    if let Some(default) = default {
        input.with_initial_text(default.to_string());
    }
    let text = input.interact_text().context("terminal input")?;
    if require && text.is_empty() {
        bail!("input cannot be empty");
    }
    Ok(text)
}

/// Ask user to edit content in editor.
pub fn edit_content<S>(cfg: &Config, raw: S, name: S, require: bool) -> Result<String>
where
    S: AsRef<str>,
{
    if cfg!(test) {
        panic!("test does not support edit content");
    }

    let editor = get_editor()?;
    let edit_path = cfg.get_meta_dir().join("tmp").join(name.as_ref());
    if !raw.as_ref().is_empty() {
        let data = raw.as_ref().to_string();
        utils::write_file(&edit_path, data.as_bytes())?;
    }
    edit_file(&editor, &edit_path)?;

    let data = fs::read(&edit_path).context("read edit file")?;
    fs::remove_file(&edit_path).context("remove edit file")?;
    let content = String::from_utf8(data).context("decode edit content as utf-8")?;
    if require && content.is_empty() {
        bail!("require edit content");
    }

    Ok(content)
}

/// Ask user to edit items in editor.
pub fn edit_items(cfg: &Config, items: Vec<String>) -> Result<Vec<String>> {
    if cfg!(test) {
        panic!("test does not support edit items");
    }

    let content = items.join("\n");
    let content = edit_content(cfg, content.as_str(), "filter_names", true)?;
    Ok(content
        .split('\n')
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect())
}

/// Ask user to edit file in editor.
pub fn edit_file(editor: impl AsRef<str>, path: &PathBuf) -> Result<()> {
    utils::ensure_dir(path)?;
    let mut cmd = Command::new(editor.as_ref());
    cmd.stdout(Stdio::inherit());
    cmd.stderr(Stdio::inherit());
    cmd.stdin(Stdio::inherit());
    cmd.arg(&path);
    cmd.output()
        .with_context(|| format!("use editor {} to edit {}", editor.as_ref(), path.display()))?;
    Ok(())
}

/// Get default editor, from env `$EDITOR`.
pub fn get_editor() -> Result<String> {
    let editor = env::var("EDITOR").context("get $EDITOR env")?;
    if editor.is_empty() {
        bail!("could not get your default editor, please set env $EDITOR");
    }
    Ok(editor)
}

/// Represents the result of a command execution, containing both the command
/// output and the return code. Different functions can be used to further process
/// the command result.
pub struct CmdResult {
    /// The return code of the command.
    pub code: Option<i32>,

    /// The piped stdout of the command.
    pub stdout: String,
    /// The piped stderr of the command, if the command is displayed, this will
    /// be empty.
    pub stderr: String,

    /// If it is [`None`], it means that the error message of the command has
    /// already been displayed, and the `Result` does not need to show the
    /// information again while handling the result. Otherwise, when handling the
    /// result, it needs to show error information, including the command output.
    pub display: Option<String>,
}

impl CmdResult {
    /// Check the execution result of the command. If command exited with none-zero
    /// return an error.
    pub fn check(&self) -> Result<()> {
        if let Some(code) = self.code {
            if code == 0 {
                return Ok(());
            }
        }
        if let None = self.display {
            // The command has already been output to the terminal, and its output
            // has been redirected. No need to print any error messages here.
            bail!(SilentExit { code: 130 });
        }
        let cmd_name = self.display.as_ref().unwrap();

        let code = match self.code {
            Some(code) => code.to_string(),
            None => String::from("<unknown>"),
        };

        let mut msg = format!("command `{cmd_name}` exited with bad code {code}");
        if !self.stdout.is_empty() {
            msg.push_str(&format!(
                ", stdout: '{}'",
                Self::trim_output(&self.stdout, true)
            ));
        }
        if !self.stderr.is_empty() {
            msg.push_str(&format!(
                ", stderr: '{}'",
                Self::trim_output(&self.stderr, true)
            ));
        }

        bail!(msg)
    }

    /// Check result and return stdout output.
    pub fn read(&self) -> Result<String> {
        self.check()?;
        Ok(Self::trim_output(&self.stdout, false))
    }

    /// Check result and split stdout output to lines.
    pub fn lines(&self) -> Result<Vec<String>> {
        let output = self.read()?;
        let lines: Vec<String> = output
            .split("\n")
            .filter_map(|line| {
                if line.is_empty() {
                    return None;
                }
                Some(line.trim().to_string())
            })
            .collect();
        Ok(lines)
    }

    fn trim_output(s: &String, no_break: bool) -> String {
        let s = s.trim();
        if no_break {
            s.replace("\n", "; ").replace("'", "")
        } else {
            String::from(s)
        }
    }
}

/// Represents a command that needs to be executed in the terminal.
///
/// It is a wrapper around [`Command`], and in addition to the original functionality,
/// it can optionally output command information for better presentation and debugging.
///
/// By default, the output of the command will be piped to prevent terminal
/// contamination. If you want to redirect the command output to the terminal, you
/// need to use [`Cmd::with_display`].
///
/// # Examples
///
/// ```
/// Cmd::new("ls").lines().unwrap();
/// Cmd::with_args("df", &["-h"]).execute_check().unwrap();
/// Cmd::git(&["status", "-s"]).with_display("Get git status").lines().unwrap();
/// Cmd::git(&["branch"]).with_display_cmd().read().unwrap();
/// ```
pub struct Cmd<'a> {
    name: &'a str,

    cmd: Command,
    input: Option<String>,

    display: Option<String>,

    script: Option<String>,
}

impl<'a> Cmd<'_> {
    /// Use `program` to create a new [`Cmd`].
    pub fn new(program: &str) -> Cmd {
        Self::with_args(program, &[])
    }

    /// Create a new [`Cmd`], with args.
    pub fn with_args(program: &'a str, args: &[&str]) -> Cmd<'a> {
        let mut cmd = Command::new(program);
        if !args.is_empty() {
            cmd.args(args);
        }

        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        Cmd {
            name: program,
            cmd,
            input: None,
            display: None,
            script: None,
        }
    }

    /// Create a new git [`Cmd`].
    pub fn git(args: &[&str]) -> Cmd<'a> {
        Self::with_args("git", args)
    }

    /// When executing a command, display the command name, args, and a prompt.
    /// If this function is called, the command's stderr will be redirected to the
    /// terminal, and if it fails during execution, it will return a [`SilentExit`].
    pub fn with_display(mut self, display: impl ToString) -> Self {
        self.display = Some(display.to_string());
        // We redirect command's stderr to program's. So that user can view command's
        // error output directly.
        self.cmd.stderr(Stdio::inherit());
        self
    }

    /// Similar to [`Cmd::with_display`], but when executed, only display the
    /// command without showing prompt information.
    pub fn with_display_cmd(self) -> Self {
        self.with_display("")
    }

    /// Set stdin as piped and pass `input` to the command's stdin.
    pub fn with_input(&mut self, input: String) -> &mut Self {
        self.input = Some(input);
        self.cmd.stdin(Stdio::piped());
        self.cmd.stderr(Stdio::inherit());
        self
    }

    /// Set environment variable for the command.
    pub fn with_env<K, V>(&mut self, key: K, val: V) -> &mut Self
    where
        K: AsRef<str>,
        V: AsRef<str>,
    {
        self.cmd.env(key.as_ref(), val.as_ref());
        self
    }

    /// Set the work directory for the command.
    pub fn with_path(&mut self, path: &PathBuf) -> &mut Self {
        self.cmd.current_dir(path);
        self
    }

    /// Create a sh command, encapsulating the command using `sh -c "xxx"`.
    ///
    /// ## Compatibility
    ///
    /// This function is only supported on Unix system.
    pub fn sh(script: &str) -> Cmd {
        let raw = script.to_string();
        // FIXME: We add `> /dev/stderr` at the end of the script to ensure that
        // the script does not output any content to stdout. This method is not
        // applicable to Windows and a more universal method is needed.
        let script = format!("{script} > /dev/stderr");
        let mut cmd = Self::with_args("sh", &["-c", script.as_str()]);
        cmd.script = Some(raw);
        cmd
    }

    /// Execute the command and return the output as multiple lines.
    /// See: [`CmdResult::lines`].
    pub fn lines(&mut self) -> Result<Vec<String>> {
        self.execute()?.lines()
    }

    /// Execute the command and return the output as string. See: [`CmdResult::read`].
    pub fn read(&mut self) -> Result<String> {
        self.execute()?.read()
    }

    /// Execute the command and check result.
    pub fn execute_check(&mut self) -> Result<()> {
        self.execute()?.check()
    }

    /// Execute the command without performing result validation.
    ///
    /// See: [`CmdResult`].
    pub fn execute(&mut self) -> Result<CmdResult> {
        let result_display = self.show();

        let mut child = match self.cmd.spawn() {
            Ok(child) => child,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                bail!(
                    "could not find command `{}`, please make sure it is installed",
                    self.name
                );
            }
            Err(e) => {
                return Err(e).with_context(|| format!("could not launch command `{}`", self.name))
            }
        };

        if let Some(input) = &self.input {
            let handle = child.stdin.as_mut().unwrap();
            if let Err(err) = write!(handle, "{}", input) {
                return Err(err).with_context(|| format!("write input to command `{}`", self.name));
            }

            drop(child.stdin.take());
        }

        let mut stdout = child.stdout.take();
        let mut stderr = child.stderr.take();

        let status = child.wait().context("Wait command done")?;
        let stdout = match stdout.as_mut() {
            Some(stdout) => {
                let mut out = String::new();
                stdout
                    .read_to_string(&mut out)
                    .with_context(|| format!("read stdout from command `{}`", self.name))?;
                out
            }
            None => String::new(),
        };
        let stderr = match stderr.as_mut() {
            Some(stderr) => {
                let mut out = String::new();
                stderr
                    .read_to_string(&mut out)
                    .with_context(|| format!("read stderr from command `{}`", self.name))?;
                out
            }
            None => String::new(),
        };

        Ok(CmdResult {
            code: status.code(),
            display: result_display,
            stdout,
            stderr,
        })
    }

    fn show(&self) -> Option<String> {
        let cmd_name = match self.script.as_ref() {
            Some(script) => format!("sh -c '{}'", style(script).underlined()),
            None => {
                let mut cmd_args = Vec::with_capacity(1);
                cmd_args.push(self.cmd.get_program().to_str().unwrap());
                let args = self.cmd.get_args();
                for arg in args {
                    cmd_args.push(arg.to_str().unwrap());
                }
                cmd_args.join(" ")
            }
        };

        if let None = self.display {
            return Some(cmd_name);
        }
        let display = self.display.as_ref().unwrap().as_str();

        if !display.is_empty() {
            exec!("{}", display);
        }
        stderr!("{} {}", style("::").cyan().bold(), cmd_name);
        None
    }
}

/// This is a simple wrapper for [`Cmd`] that allows setting the working directory.
/// For GitCmd, this is achieved using args `-C <work-dir>`.
///
/// # Examples
///
/// ```
/// GitCmd::with_path("/path/to/repo").exec(&["checkout", "main"]).unwrap();
/// GitCmd::with_path("/path/to/repo").lines(&["branch", "-vv"]).unwrap();
/// GitCmd::with_path("/path/to/repo").checkout("v0.1.0").unwrap();
/// ```
pub struct GitCmd<'a> {
    prefix: Vec<&'a str>,
}

impl<'a> GitCmd<'a> {
    /// Create a [`GitCmd`] and set the working directory at the same time.
    pub fn with_path(path: &'a str) -> GitCmd<'a> {
        let prefix = if path != "" { vec!["-C", path] } else { vec![] };
        GitCmd { prefix }
    }

    /// See: [`Cmd::execute`].
    pub fn exec(&self, args: &[&str]) -> Result<()> {
        let args = [&self.prefix, args].concat();
        Cmd::git(&args).execute()?.check()
    }

    /// See: [`Cmd::lines`].
    pub fn lines(&self, args: &[&str]) -> Result<Vec<String>> {
        let args = [&self.prefix, args].concat();
        Cmd::git(&args).execute()?.lines()
    }

    /// See: [`Cmd::read`].
    pub fn read(&self, args: &[&str]) -> Result<String> {
        let args = [&self.prefix, args].concat();
        Cmd::git(&args).execute()?.read()
    }

    /// Use `checkout` to switch to a new branch, tag, or commit. This is a wrapper
    /// for the command `git checkout <target>`.
    pub fn checkout(&self, arg: &str) -> Result<()> {
        self.exec(&["checkout", arg])
    }
}

/// Use the `fzf` command to search through multiple items. Return the index of the
/// selected item from the search results.
///
/// # Examples
///
/// ```
/// let items = vec!["item0", "item1", "item2"];
/// let idx = fzf_search(&items).unwrap();
/// let result = items[idx];
/// ```
pub fn fzf_search<S>(keys: &Vec<S>) -> Result<usize>
where
    S: AsRef<str>,
{
    let mut input = String::with_capacity(keys.len());
    for key in keys {
        input.push_str(key.as_ref());
        input.push_str("\n");
    }

    let mut fzf = Cmd::new("fzf");
    fzf.with_input(input);

    let result = fzf.execute()?;
    match result.code {
        Some(0) => {
            let output = result.read()?;
            match keys.iter().position(|s| s.as_ref() == output) {
                Some(idx) => Ok(idx),
                None => bail!("could not find key {}", output),
            }
        }
        Some(1) => bail!("fzf no match found"),
        Some(2) => bail!("fzf returned an error"),
        Some(130) => bail!(SilentExit { code: 130 }),
        Some(128..=254) | None => bail!("fzf was terminated"),
        _ => bail!("fzf returned an unknown error"),
    }
}

/// If there are uncommitted changes in the current Git repository, return an error.
/// This will use `git status -s` to check.
pub fn ensure_no_uncommitted() -> Result<()> {
    let mut git =
        Cmd::git(&["status", "-s"]).with_display("Check if repository has uncommitted change");
    let lines = git.lines()?;
    if !lines.is_empty() {
        bail!(
            "you have {} to handle",
            utils::plural(&lines, "uncommitted change"),
        );
    }
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
pub enum BranchStatus {
    Sync,
    Gone,
    Ahead,
    Behind,
    Conflict,
    Detached,
}

impl BranchStatus {
    pub fn display(&self) -> StyledObject<&'static str> {
        match self {
            Self::Sync => style("sync").green(),
            Self::Gone => style("gone").red(),
            Self::Ahead => style("ahead").yellow(),
            Self::Behind => style("behind").yellow(),
            Self::Conflict => style("conflict").yellow().bold(),
            Self::Detached => style("detached").red(),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct GitBranch {
    pub name: String,
    pub status: BranchStatus,

    pub current: bool,
}

impl GitBranch {
    const BRANCH_REGEX: &'static str = r"^(\*)*[ ]*([^ ]*)[ ]*([^ ]*)[ ]*(\[[^\]]*\])*[ ]*(.*)$";
    const HEAD_BRANCH_PREFIX: &'static str = "HEAD branch:";

    pub fn get_regex() -> Regex {
        Regex::new(Self::BRANCH_REGEX).expect("parse git branch regex")
    }

    pub fn list() -> Result<Vec<GitBranch>> {
        let re = Self::get_regex();
        let lines = Cmd::git(&["branch", "-vv"]).lines()?;
        let mut branches: Vec<GitBranch> = Vec::with_capacity(lines.len());
        for line in lines {
            let branch = Self::parse(&re, line)?;
            branches.push(branch);
        }

        Ok(branches)
    }

    pub fn list_remote(remote: &str) -> Result<Vec<String>> {
        let lines = Cmd::git(&["branch", "-al"])
            .with_display("List remote git branch")
            .lines()?;
        let remote_prefix = format!("{remote}/");
        let mut items = Vec::with_capacity(lines.len());
        for line in lines {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if !line.starts_with("remotes/") {
                continue;
            }
            let item = line.strip_prefix("remotes/").unwrap();
            if !item.starts_with(&remote_prefix) {
                continue;
            }
            let item = item.strip_prefix(&remote_prefix).unwrap().trim();
            if item.is_empty() {
                continue;
            }
            if item.starts_with("HEAD ->") {
                continue;
            }
            items.push(item.to_string());
        }

        let lines = Cmd::git(&["branch"])
            .with_display("List local git branch")
            .lines()?;
        let mut local_branch_map = HashSet::with_capacity(lines.len());
        for line in lines {
            let mut line = line.trim();
            if line.is_empty() {
                continue;
            }
            if line.starts_with("*") {
                line = line.strip_prefix("*").unwrap().trim();
            }
            local_branch_map.insert(line.to_string());
        }

        Ok(items
            .into_iter()
            .filter(|item| !local_branch_map.contains(item))
            .collect())
    }

    pub fn default() -> Result<String> {
        Self::default_by_remote("origin")
    }

    pub fn default_by_remote(remote: &str) -> Result<String> {
        info!("Get default branch for {}", remote);
        let head_ref = format!("refs/remotes/{}/HEAD", remote);
        let remote_ref = format!("refs/remotes/{}/", remote);

        let mut git = Cmd::git(&["symbolic-ref", &head_ref]).with_display_cmd();
        if let Ok(out) = git.read() {
            if out.is_empty() {
                bail!("default branch is empty")
            }
            return match out.strip_prefix(&remote_ref) {
                Some(s) => Ok(s.to_string()),
                None => bail!("invalid ref output by git: {}", style(out).yellow()),
            };
        }
        // If failed, user might not switch to this branch yet, let's
        // use "git show <remote>" instead to get default branch.
        let mut git = Cmd::git(&["remote", "show", remote]).with_display_cmd();
        let lines = git.lines()?;
        Self::parse_default_branch(lines)
    }

    pub fn parse_default_branch(lines: Vec<String>) -> Result<String> {
        for line in lines {
            if let Some(branch) = line.trim().strip_prefix(Self::HEAD_BRANCH_PREFIX) {
                let branch = branch.trim();
                if branch.is_empty() {
                    bail!("default branch returned by git remote show is empty");
                }
                return Ok(branch.to_string());
            }
        }

        bail!("no default branch returned by git remote show, please check your git command");
    }

    pub fn current() -> Result<String> {
        Cmd::git(&["branch", "--show-current"])
            .with_display("Get current branch info")
            .read()
    }

    pub fn parse(re: &Regex, line: impl AsRef<str>) -> Result<GitBranch> {
        let parse_err = format!(
            "invalid branch description {}, please check your git command",
            style(line.as_ref()).yellow()
        );
        let mut iter = re.captures_iter(line.as_ref());
        let caps = match iter.next() {
            Some(caps) => caps,
            None => bail!(parse_err),
        };
        // We have 6 captures:
        //   0 -> line itself
        //   1 -> current branch
        //   2 -> branch name
        //   3 -> commit id
        //   4 -> remote description
        //   5 -> commit message
        if caps.len() != 6 {
            bail!(parse_err)
        }
        let mut current = false;
        if let Some(_) = caps.get(1) {
            current = true;
        }

        let name = match caps.get(2) {
            Some(name) => name.as_str().trim(),
            None => bail!("{}: missing name", parse_err),
        };

        let status = match caps.get(4) {
            Some(remote_desc) => {
                let remote_desc = remote_desc.as_str();
                let behind = remote_desc.contains("behind");
                let ahead = remote_desc.contains("ahead");

                if remote_desc.contains("gone") {
                    BranchStatus::Gone
                } else if ahead && behind {
                    BranchStatus::Conflict
                } else if ahead {
                    BranchStatus::Ahead
                } else if behind {
                    BranchStatus::Behind
                } else {
                    BranchStatus::Sync
                }
            }
            None => BranchStatus::Detached,
        };

        Ok(GitBranch {
            name: name.to_string(),
            status,
            current,
        })
    }
}

pub struct GitRemote(String);

impl GitRemote {
    pub fn list() -> Result<Vec<GitRemote>> {
        let lines = Cmd::git(&["remote"])
            .with_display("List git remotes")
            .lines()?;
        Ok(lines.iter().map(|s| GitRemote(s.to_string())).collect())
    }

    pub fn new() -> GitRemote {
        GitRemote(String::from("origin"))
    }

    pub fn from_upstream(
        cfg: &Config,
        repo: &Rc<Repo>,
        provider: &Box<dyn Provider>,
    ) -> Result<GitRemote> {
        let remotes = Self::list()?;
        let upstream_remote = remotes
            .into_iter()
            .find(|remote| remote.0.as_str() == "upstream");
        if let Some(remote) = upstream_remote {
            return Ok(remote);
        }

        info!("Get upstream for {}", repo.name_with_remote());
        let api_repo = provider.get_repo(&repo.owner.name, &repo.name)?;
        if let None = api_repo.upstream {
            bail!(
                "repo {} is not forked, so it has not an upstream",
                repo.name_with_remote()
            );
        }
        let api_upstream = api_repo.upstream.unwrap();
        let upstream = Repo::from_api_upstream(cfg, &repo.remote.name, api_upstream)?;
        let url = upstream.clone_url();

        confirm!(
            "Do you want to set upstream to {}: {}",
            upstream.name_with_remote(),
            url
        );

        Cmd::git(&["remote", "add", "upstream", url.as_str()])
            .execute()?
            .check()?;
        Ok(GitRemote(String::from("upstream")))
    }

    pub fn target(&self, branch: Option<&str>) -> Result<String> {
        let (target, branch) = match branch {
            Some(branch) => (format!("{}/{}", self.0, branch), branch.to_string()),
            None => {
                let branch = GitBranch::default_by_remote(&self.0)?;
                (format!("{}/{}", self.0, branch), branch)
            }
        };
        Cmd::git(&["fetch", self.0.as_str(), branch.as_str()]).execute_check()?;
        Ok(target)
    }

    pub fn commits_between(&self, branch: Option<&str>) -> Result<Vec<String>> {
        let target = self.target(branch)?;
        let compare = format!("HEAD...{}", target);
        let lines = Cmd::git(&[
            "log",
            "--left-right",
            "--cherry-pick",
            "--oneline",
            compare.as_str(),
        ])
        .with_display(format!("Get commits between {target}"))
        .lines()?;
        let commits: Vec<_> = lines
            .iter()
            .filter(|line| {
                // If the commit message output by "git log xxx" does not start
                // with "<", it means that this commit is from the target branch.
                // Since we only list commits from current branch, ignore such
                // commits.
                line.trim().starts_with("<")
            })
            .map(|line| line.strip_prefix("<").unwrap().to_string())
            .map(|line| {
                let mut fields = line.split_whitespace();
                fields.next();
                let commit = fields
                    .map(|field| field.to_string())
                    .collect::<Vec<String>>()
                    .join(" ");
                commit
            })
            .collect();
        Ok(commits)
    }

    pub fn get_url(&self) -> Result<String> {
        let url = Cmd::git(&["remote", "get-url", &self.0])
            .with_display(format!("Get url for remote {}", self.0))
            .read()?;
        Ok(url)
    }
}

pub struct GitTag(pub String);

impl std::fmt::Display for GitTag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl GitTag {
    const NUM_REGEX: &'static str = r"\d+";
    const PLACEHOLDER_REGEX: &'static str = r"\{(\d+|%[yYmMdD])(\+)*}";

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    pub fn list() -> Result<Vec<GitTag>> {
        let tags: Vec<_> = Cmd::git(&["tag"])
            .with_display("Get git tags")
            .lines()?
            .iter()
            .filter(|line| !line.trim().is_empty())
            .map(|line| GitTag(line.trim().to_string()))
            .collect();
        Ok(tags)
    }

    pub fn get(s: impl AsRef<str>) -> Result<GitTag> {
        let tags = Self::list()?;
        for tag in tags {
            if tag.as_str() == s.as_ref() {
                return Ok(tag);
            }
        }
        bail!("could not find tag '{}'", s.as_ref())
    }

    pub fn latest() -> Result<GitTag> {
        Cmd::git(&["fetch", "origin", "--prune-tags"])
            .with_display("Fetch tags")
            .execute_check()?;
        let output = Cmd::git(&["describe", "--tags", "--abbrev=0"])
            .with_display("Get latest tag")
            .read()?;
        if output.is_empty() {
            bail!("no latest tag");
        }
        Ok(GitTag(output))
    }

    pub fn apply_rule(&self, rule: impl AsRef<str>) -> Result<GitTag> {
        let num_re = Regex::new(Self::NUM_REGEX).context("unable to parse num regex")?;
        let ph_re = Regex::new(Self::PLACEHOLDER_REGEX)
            .context("unable to parse rule placeholder regex")?;

        let nums: Vec<i32> = num_re
            .captures_iter(self.0.as_str())
            .map(|caps| {
                caps.get(0)
                    .unwrap()
                    .as_str()
                    .parse()
                    // The caps here MUST be number, so it is safe to use expect here
                    .expect("unable to parse num caps")
            })
            .collect();

        let mut with_date = false;
        let result = ph_re.replace_all(rule.as_ref(), |caps: &Captures| {
            let rep = caps.get(1).unwrap().as_str();
            let idx: usize = match rep.parse() {
                Ok(idx) => idx,
                Err(_) => {
                    with_date = true;
                    return rep.to_string();
                }
            };
            let num = if idx >= nums.len() { 0 } else { nums[idx] };
            let plus = caps.get(2);
            match plus {
                Some(_) => format!("{}", num + 1),
                None => format!("{}", num),
            }
        });
        let mut result = result.to_string();
        if with_date {
            let date = Local::now();
            result = date.format(&result).to_string();
        }

        Ok(GitTag(result))
    }
}

pub struct Workflow {
    pub name: String,
    steps: Vec<WorkflowStep>,

    cfg: Config,
}

impl Workflow {
    pub fn new(cfg: &Config, name: impl AsRef<str>) -> Result<Workflow> {
        match cfg.workflows.get(name.as_ref()) {
            Some(steps) => Ok(Workflow {
                name: name.as_ref().to_string(),
                steps: steps.clone(),
                cfg: cfg.clone(),
            }),
            None => bail!("could not find workflow '{}'", name.as_ref()),
        }
    }

    pub fn execute_repo(&self, repo: &Rc<Repo>) -> Result<()> {
        info!(
            "Execute workflow '{}' for '{}'",
            self.name,
            repo.name_with_remote()
        );
        let dir = repo.get_path(&self.cfg);
        for step in self.steps.iter() {
            if let Some(run) = &step.run {
                let script = run.replace("\n", ";");
                let mut cmd = Cmd::sh(&script).with_display(format!("{}", step.name));
                cmd.with_path(&dir);

                cmd.with_env("REPO_NAME", repo.name.as_str());
                cmd.with_env("REPO_OWNER", repo.owner.name.as_str());
                cmd.with_env("REMOTE", repo.remote.name.as_str());
                cmd.with_env("REPO_NAME_WITH_OWNER", repo.name_with_owner());
                cmd.with_env("REPO_NAME_WITH_REMOTE", repo.name_with_remote());

                cmd.execute_check()?;
                continue;
            }
            if let None = step.file {
                continue;
            }

            let content = step.file.as_ref().unwrap();
            let content = content.replace("\\t", "\t");

            exec!("Create file '{}'", step.name);
            let path = dir.join(&step.name);
            utils::write_file(&path, content.as_bytes())?;
        }
        Ok(())
    }

    pub fn execute_task<S>(&self, dir: &PathBuf, remote: S, owner: S, name: S) -> Result<()>
    where
        S: AsRef<str>,
    {
        let long_name = format!("{}/{}", owner.as_ref(), name.as_ref());
        let full_name = format!("{}:{}/{}", remote.as_ref(), owner.as_ref(), name.as_ref());

        for step in self.steps.iter() {
            if let Some(run) = &step.run {
                let script = run.replace("\n", ";");
                let mut cmd = Cmd::sh(&script);
                cmd.with_path(&dir);

                cmd.with_env("REPO_NAME", name.as_ref());
                cmd.with_env("REPO_OWNER", owner.as_ref());
                cmd.with_env("REMOTE", remote.as_ref());
                cmd.with_env("REPO_LONG", long_name.as_str());
                cmd.with_env("REPO_FULL", full_name.as_str());

                cmd.execute_check()?;
                continue;
            }
            if let None = step.file {
                continue;
            }

            let content = step.file.as_ref().unwrap();
            let content = content.replace("\\t", "\t");

            let path = dir.join(&step.name);
            utils::write_file(&path, content.as_bytes())?;
        }
        Ok(())
    }
}

pub struct Table {
    ncol: usize,
    rows: Vec<Vec<String>>,
}

impl Table {
    pub fn with_capacity(size: usize) -> Table {
        Table {
            ncol: 0,
            rows: Vec::with_capacity(size),
        }
    }

    pub fn add(&mut self, row: Vec<String>) {
        if row.is_empty() {
            panic!("empty row");
        }
        if self.ncol == 0 {
            self.ncol = row.len();
        } else if row.len() != self.ncol {
            panic!("unexpect row len");
        }
        self.rows.push(row);
    }

    pub fn show(self) {
        let mut pads = Vec::with_capacity(self.ncol);
        for i in 0..self.ncol {
            let mut max_len: usize = 0;
            for row in self.rows.iter() {
                let cell = row.get(i).unwrap();
                if cell.len() > max_len {
                    max_len = cell.len()
                }
            }
            pads.push(max_len + 2);
        }

        for row in self.rows.into_iter() {
            for (i, cell) in row.into_iter().enumerate() {
                let pad = pads[i];
                let cell = cell.pad_to_width_with_alignment(pad, pad::Alignment::Left);
                print!("{}", cell);
            }
            println!()
        }
    }
}

struct ProgressWriter<W: Write> {
    desc: String,
    desc_size: usize,

    upstream: W,

    last_report: Instant,

    current: usize,
    total: usize,
}

impl<W: Write> ProgressWriter<W> {
    const SPACE: &'static str = " ";
    const SPACE_SIZE: usize = 1;

    const REPORT_INTERVAL: Duration = Duration::from_millis(200);

    pub fn new(desc: String, total: usize, upstream: W) -> ProgressWriter<W> {
        let desc_size = console::measure_text_width(&desc);
        let last_report = Instant::now();

        let pw = ProgressWriter {
            desc,
            desc_size,
            upstream,
            last_report,
            current: 0,
            total,
        };
        write_stderr(pw.render());
        pw
    }

    fn render(&self) -> String {
        let term_size = size();
        if self.desc_size > term_size {
            return ".".repeat(term_size);
        }

        let mut line = self.desc.clone();
        if self.desc_size + Self::SPACE_SIZE > term_size || bar_size() == 0 {
            return line;
        }
        line.push_str(Self::SPACE);

        let bar = render_bar(self.current, self.total);
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
        line
    }
}

impl<W: Write> Write for ProgressWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let size = self.upstream.write(buf)?;
        self.current += size;

        if self.current >= self.total {
            self.current = self.total;
            cursor_up();
            write_stderr(self.render());
            return Ok(size);
        }

        let now = Instant::now();
        let delta = now - self.last_report;
        if delta >= Self::REPORT_INTERVAL {
            cursor_up();
            write_stderr(self.render());
            self.last_report = now;
        }

        Ok(size)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.upstream.flush()
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

    let mut pw = ProgressWriter::new(desc, total as usize, file);
    resp.copy_to(&mut pw).context("download data")?;

    cursor_up();
    info!("Download {} done", name);

    Ok(())
}

#[cfg(test)]
mod term_tests {
    use crate::term::*;

    #[test]
    fn test_show_json() {
        #[derive(Debug, Serialize)]
        struct TestInfo {
            pub name: String,
            pub age: i32,
            pub emails: Vec<String>,
        }

        let info = TestInfo {
            name: String::from("Rowlet"),
            age: 25,
            emails: vec![
                String::from("lazycat7706@gmail.com"),
                String::from("631029386@qq.com"),
            ],
        };
        show_json(&info).unwrap();
    }

    #[test]
    fn test_parse_branch() {
        let cases = vec![
            (
                "* main cf11adb [origin/main] My best commit since the project begin",
                "main",
                BranchStatus::Sync,
                true,
            ),
            (
                "release/1.6 dc07e7ec7 [origin/release/1.6] Merge pull request #9024 from akhilerm/cherry-pick-9021-release/1.6",
                "release/1.6",
                BranchStatus::Sync,
                false
            ),
            (
                "feat/update-version 3b0569d62 [origin/feat/update-version: ahead 1] chore: update cargo version",
                "feat/update-version",
                BranchStatus::Ahead,
                false
            ),
            (
                "* feat/tmp-dev 92bbd6e [origin/feat/tmp-dev: gone] Merge pull request #6 from fioncat/hello",
                "feat/tmp-dev",
                BranchStatus::Gone,
                true
            ),
            (
                "master       b4a40de [origin/master: ahead 1, behind 1] test commit",
                "master",
                BranchStatus::Conflict,
                false
            ),
            (
                "* dev        b4a40de test commit",
                "dev",
                BranchStatus::Detached,
                true
            ),
        ];

        let re = GitBranch::get_regex();
        for (raw, name, status, current) in cases {
            let result = GitBranch::parse(&re, raw).unwrap();
            let expect = GitBranch {
                name: String::from(name),
                status,
                current,
            };
            assert_eq!(result, expect);
        }
    }

    #[test]
    fn test_parse_default_branch() {
        // Lines from `git remote show origin`
        let lines = vec![
            "  * remote origin",
            "  Fetch URL: git@github.com:fioncat/roxide.git",
            "  Push  URL: git@github.com:fioncat/roxide.git",
            "  HEAD branch: main",
            "  Remote branches:",
            "    feat/test tracked",
            "    main      tracked",
            "  Local branches configured for 'git pull':",
            "    feat/test merges with remote feat/test",
            "    main      merges with remote main",
            "  Local refs configured for 'git push':",
            "    feat/test pushes to feat/test (up to date)",
            "    main      pushes to main      (up to date)",
        ];

        let default_branch = GitBranch::parse_default_branch(
            lines.into_iter().map(|line| line.to_string()).collect(),
        )
        .unwrap();
        assert_eq!(default_branch, "main");
    }

    #[test]
    fn test_apply_tag_rule() {
        let cases = vec![
            ("v0.1.0", "v{0}.{1+}.0", "v0.2.0"),
            ("v12.1.0", "v{0+}.0.0", "v13.0.0"),
            ("v0.8.13", "{0}.{1}.{2+}-rc1", "0.8.14-rc1"),
            ("1.2.23", "{0}.{1}.{2+}-rc{3}", "1.2.24-rc0"),
            ("1.2.23", "{0}.{1}.{2+}-rc{3+}", "1.2.24-rc1"),
            ("1.2.20-rc4", "{0}.{1}.{2+}-rc{3+}", "1.2.21-rc5"),
        ];

        for (before, rule, expect) in cases {
            let tag = GitTag(String::from(before));
            let result = tag.apply_rule(rule).unwrap();
            assert_eq!(result.as_str(), expect);
        }
    }
}

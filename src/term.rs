use std::collections::HashSet;
use std::env;
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::mem;
use std::path::PathBuf;
use std::process::{ChildStdout, Command, Stdio};
use std::rc::Rc;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use chrono::offset::Local;
use console::{style, StyledObject, Term};
use dialoguer::theme::ColorfulTheme;
use dialoguer::Input;
use pad::PadStr;
use regex::{Captures, Regex};
use reqwest::blocking::Client;
use reqwest::{Method, Url};
use semver::Version;
use serde::Serialize;
use serde_json::ser::PrettyFormatter;
use serde_json::Serializer;

use crate::api::types::Provider;
use crate::config::types::{Remote, WorkflowStep};
use crate::errors::SilentExit;
use crate::repo::types::Repo;
use crate::{config, utils};
use crate::{confirm, exec, info};

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

pub fn show_exec(msg: impl AsRef<str>) {
    let msg = format!("{} {}", style("==>").cyan(), msg.as_ref());
    write_stderr(msg);
}

pub fn show_info(msg: impl AsRef<str>) {
    let msg = format!("{} {}", style("==>").green(), msg.as_ref());
    write_stderr(msg);
}

pub fn write_stderr(msg: String) {
    _ = writeln!(std::io::stderr(), "{}", msg);
}

pub fn show_json<T: Serialize>(value: T) -> Result<()> {
    let formatter = PrettyFormatter::with_indent(b"  ");
    let mut buf = Vec::new();
    let mut ser = Serializer::with_formatter(&mut buf, formatter);
    value.serialize(&mut ser).context("Serialize object")?;
    let json = String::from_utf8(buf).context("UTF8 encode json")?;
    println!("{json}");
    Ok(())
}

pub const CURSOR_UP_CHARS: &str = "\x1b[A\x1b[K";

pub fn cursor_up_stderr() {
    _ = write!(std::io::stderr(), "{}", CURSOR_UP_CHARS);
}

pub fn cursor_up_stdout() {
    print!("{CURSOR_UP_CHARS}");
}

pub fn size() -> usize {
    let term = Term::stdout();
    let (_, col_size) = term.size();
    col_size as usize
}

pub fn bar_size() -> usize {
    let term_size = size();
    if term_size <= 20 {
        0
    } else {
        term_size / 4
    }
}

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

pub fn confirm(msg: impl AsRef<str>) -> Result<bool> {
    let msg = format!(":: {}?", msg.as_ref());
    _ = write!(std::io::stderr(), "{} [Y/n] ", style(msg).bold());

    let mut answer = String::new();
    scanf::scanf!("{}", answer).context("confirm: Scan terminal stdin")?;
    if answer.to_lowercase() != "y" {
        return Ok(false);
    }

    return Ok(true);
}

pub fn must_confirm(msg: impl AsRef<str>) -> Result<()> {
    let ok = confirm(msg)?;
    if !ok {
        bail!(SilentExit { code: 60 });
    }
    Ok(())
}

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

pub fn confirm_items(
    items: &Vec<String>,
    action: &str,
    noun: &str,
    name: &str,
    plural: &str,
) -> Result<bool> {
    if items.is_empty() {
        println!("Nothing to {action}");
        return Ok(false);
    }

    info!("Require confirm to {}", action);
    println!();

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
                print!("{head}{item}");
            } else {
                print!("{head_space}{item}");
            }
            current_size = head_size + item_size;
            continue;
        }

        current_size += 2 + item_size;
        if current_size > col_size {
            println!();
            print!("{head_space}{item}");
            current_size = head_size + item_size;
            continue;
        }

        print!("  {item}");
    }
    println!("\n");

    println!(
        "Total {} {} to {}",
        items.len(),
        name.to_lowercase(),
        action
    );
    println!();
    let ok = confirm(format!("Proceed with {noun}"))?;
    if ok {
        println!();
    }
    Ok(ok)
}

pub fn input(msg: impl AsRef<str>, require: bool, default: Option<&str>) -> Result<String> {
    let theme = ColorfulTheme::default();
    let mut input: Input<String> = Input::with_theme(&theme);
    input.with_prompt(msg.as_ref());
    if let Some(default) = default {
        input.with_initial_text(default.to_string());
    }
    let text = input.interact_text().context("Terminal input")?;
    if require && text.is_empty() {
        bail!("Input cannot be empty");
    }
    Ok(text)
}

pub fn edit_content<S>(raw: S, name: S, require: bool) -> Result<String>
where
    S: AsRef<str>,
{
    let editor = get_editor()?;
    let edit_path = PathBuf::from(&config::base().metadir)
        .join("tmp")
        .join(name.as_ref());
    if !raw.as_ref().is_empty() {
        let data = raw.as_ref().to_string();
        utils::write_file(&edit_path, data.as_bytes())?;
    }
    edit_file(&editor, &edit_path)?;

    let data = fs::read(&edit_path).context("Read edit file")?;
    fs::remove_file(&edit_path).context("Remove edit file")?;
    let content = String::from_utf8(data).context("Decode edit content as utf-8")?;
    if require && content.is_empty() {
        bail!("Require edit content");
    }

    Ok(content)
}

pub fn edit_items(items: Vec<String>) -> Result<Vec<String>> {
    let content = items.join("\n");
    let content = edit_content(content.as_str(), "filter_names", true)?;
    Ok(content
        .split('\n')
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect())
}

pub fn edit_file(editor: impl AsRef<str>, path: &PathBuf) -> Result<()> {
    utils::ensure_dir(path)?;
    let mut cmd = Command::new(editor.as_ref());
    cmd.stdout(Stdio::inherit());
    cmd.stderr(Stdio::inherit());
    cmd.stdin(Stdio::inherit());
    cmd.arg(&path);
    cmd.output()
        .with_context(|| format!("Use editor {} to edit {}", editor.as_ref(), path.display()))?;
    Ok(())
}

pub fn get_editor() -> Result<String> {
    let editor = env::var("EDITOR").context("Get EDITOR env")?;
    if editor.is_empty() {
        bail!("Could not get your default editor, please check env `EDITOR`");
    }
    Ok(editor)
}

pub fn git_version() -> Result<Version> {
    let version = Cmd::exec_git_mute_read(&["version"])?;
    let version = match version.trim().strip_prefix("git version") {
        Some(v) => v.trim(),
        None => bail!("Unknown git version output: {version}"),
    };

    match Version::parse(&version) {
        Ok(ver) => Ok(ver),
        Err(_) => bail!("git version {version} is bad format"),
    }
}

pub fn fzf_version() -> Result<Version> {
    let version = Cmd::exec_mute_read("fzf", &["--version"])?;
    let mut fields = version.split(" ");
    let version = match fields.next() {
        Some(v) => v.trim(),
        None => bail!("Unknown fzf version output: {version}"),
    };
    match Version::parse(&version) {
        Ok(ver) => Ok(ver),
        Err(_) => bail!("fzf version {version} is bad format"),
    }
}

pub fn shell_type() -> Result<String> {
    let shell = env::var("SHELL").context("Get SHELL env")?;
    if shell.is_empty() {
        bail!("env SHELL is empty");
    }
    let path = PathBuf::from(&shell);
    match path.file_name() {
        Some(name) => match name.to_str() {
            Some(name) => Ok(String::from(name)),
            None => bail!("Bad SHELL format: {shell}"),
        },
        None => bail!("Bad SHELL env: {shell}"),
    }
}

pub struct Cmd {
    cmd: Command,
    desc: Option<String>,
    input: Option<String>,

    mute: bool,
}

pub struct CmdResult {
    pub code: Option<i32>,

    pub stdout: ChildStdout,
}

impl CmdResult {
    pub fn read(&mut self) -> Result<String> {
        let mut output = String::new();
        self.stdout
            .read_to_string(&mut output)
            .context("Read stdout from command")?;
        Ok(output.trim().to_string())
    }

    pub fn check(&self) -> Result<()> {
        match self.code {
            Some(0) => Ok(()),
            _ => bail!(SilentExit { code: 130 }),
        }
    }

    pub fn checked_read(&mut self) -> Result<String> {
        self.check()?;
        self.read()
    }

    pub fn checked_lines(&mut self) -> Result<Vec<String>> {
        let output = self.checked_read()?;
        let lines: Vec<String> = output
            .split("\n")
            .filter_map(|line| {
                if line.is_empty() {
                    return None;
                }
                Some(line.to_string())
            })
            .collect();
        Ok(lines)
    }
}

impl Cmd {
    pub fn new(program: &str) -> Cmd {
        Self::with_args(program, &[])
    }

    pub fn with_args(program: &str, args: &[&str]) -> Cmd {
        let mut cmd = Command::new(program);
        if !args.is_empty() {
            cmd.args(args);
        }
        // The stdout will be captured anyway to protected roxide's own output.
        // If cmd fails, the caller should handle its stdout manually.
        cmd.stdout(Stdio::piped());
        // We rediect command's stderr to roxide's. So that user can view command's
        // error output directly.
        cmd.stderr(Stdio::inherit());

        Cmd {
            cmd,
            desc: None,
            input: None,
            mute: false,
        }
    }

    pub fn git(args: &[&str]) -> Cmd {
        Self::with_args("git", args)
    }

    pub fn exec_mute(program: &str, args: &[&str]) -> Result<()> {
        if Self::with_args(program, args)
            .piped_stderr()
            .set_mute(true)
            .execute()?
            .check()
            .is_err()
        {
            let cmd = args.join(" ");
            bail!("Execute command failed: {} {}", program, cmd);
        }

        Ok(())
    }

    pub fn exec_mute_lines(program: &str, args: &[&str]) -> Result<Vec<String>> {
        let result = Self::with_args(program, args)
            .piped_stderr()
            .set_mute(true)
            .execute()?
            .checked_lines();
        match result {
            Ok(lines) => Ok(lines),
            Err(_) => {
                let cmd = args.join(" ");
                bail!("Execute command failed: {} {}", program, cmd)
            }
        }
    }

    pub fn exec_mute_read(program: &str, args: &[&str]) -> Result<String> {
        let result = Self::with_args(program, args)
            .piped_stderr()
            .set_mute(true)
            .execute()?
            .checked_read();
        match result {
            Ok(s) => Ok(s),
            Err(_) => {
                let cmd = args.join(" ");
                bail!("Execute command failed: {} {}", program, cmd)
            }
        }
    }

    pub fn exec_git_mute_lines(args: &[&str]) -> Result<Vec<String>> {
        Self::exec_mute_lines("git", args)
    }

    pub fn exec_git_mute_read(args: &[&str]) -> Result<String> {
        Self::exec_mute_read("git", args)
    }

    pub fn exec_git_mute(args: &[&str]) -> Result<()> {
        Self::exec_mute("git", args)
    }

    pub fn sh(script: &str) -> Cmd {
        // FIXME: We add `> /dev/stderr` at the end of the script to ensure that
        // the script does not output any content to stdout. This method is not
        // applicable to Windows and a more universal method is needed.
        let script = format!("{script} > /dev/stderr");
        Self::with_args("sh", &["-c", script.as_str()])
    }

    pub fn piped_stderr(&mut self) -> &mut Self {
        self.cmd.stderr(Stdio::piped());
        self
    }

    pub fn with_desc(&mut self, desc: impl AsRef<str>) -> &mut Self {
        self.desc = Some(desc.as_ref().to_string());
        self
    }

    pub fn with_input(&mut self, input: String) -> &mut Self {
        self.input = Some(input);
        self.cmd.stdin(Stdio::piped());
        self
    }

    pub fn with_path(&mut self, path: &PathBuf) -> &mut Self {
        self.cmd.current_dir(path);
        self
    }

    pub fn with_env<K, V>(&mut self, key: K, val: V) -> &mut Self
    where
        K: AsRef<str>,
        V: AsRef<str>,
    {
        self.cmd.env(key.as_ref(), val.as_ref());
        self
    }

    pub fn set_mute(&mut self, mute: bool) -> &mut Self {
        self.mute = mute;
        self
    }

    pub fn execute(&mut self) -> Result<CmdResult> {
        self.show_desc();

        let mut child = match self.cmd.spawn() {
            Ok(child) => child,
            Err(e) if e.kind() == ErrorKind::NotFound => {
                bail!(
                    "Could not find command `{}`, please make sure it is installed",
                    self.cmd.get_program().to_str().unwrap()
                );
            }
            Err(e) => {
                return Err(e).with_context(|| {
                    format!(
                        "could not launch `{}`",
                        self.cmd.get_program().to_str().unwrap()
                    )
                })
            }
        };

        if let Some(input) = &self.input {
            let handle = child.stdin.as_mut().unwrap();
            if let Err(err) = write!(handle, "{}", input) {
                return Err(err).context("Write content to command");
            }

            mem::drop(child.stdin.take());
        }

        let stdout = child.stdout.take().unwrap();
        let status = child.wait().context("Wait command done")?;

        Ok(CmdResult {
            code: status.code(),
            stdout,
        })
    }

    fn show_desc(&self) {
        if self.mute {
            return;
        }
        match &self.desc {
            Some(desc) => exec!(desc),
            None => {
                let mut desc_args = Vec::with_capacity(1);
                desc_args.push(self.cmd.get_program().to_str().unwrap());
                let args = self.cmd.get_args();
                for arg in args {
                    desc_args.push(arg.to_str().unwrap());
                }
                let desc = desc_args.join(" ");
                exec!(desc);
            }
        }
    }
}

pub struct GitTask<'a> {
    prefix: Vec<&'a str>,
}

impl<'a> GitTask<'a> {
    pub fn new(path: &'a str) -> GitTask<'a> {
        let prefix = vec!["-C", path];
        GitTask { prefix }
    }

    pub fn exec(&self, args: &[&str]) -> Result<()> {
        let args = [&self.prefix, args].concat();
        Cmd::exec_git_mute(&args)
    }

    pub fn lines(&self, args: &[&str]) -> Result<Vec<String>> {
        let args = [&self.prefix, args].concat();
        Cmd::exec_git_mute_lines(&args)
    }

    pub fn read(&self, args: &[&str]) -> Result<String> {
        let args = [&self.prefix, args].concat();
        Cmd::exec_git_mute_read(&args)
    }

    pub fn checkout(&self, arg: &str) -> Result<()> {
        self.exec(&["checkout", arg])
    }
}

pub fn search<S>(keys: &Vec<S>) -> Result<usize>
where
    S: AsRef<str>,
{
    let mut input = String::with_capacity(keys.len());
    for key in keys {
        input.push_str(key.as_ref());
        input.push_str("\n");
    }

    let mut fzf = Cmd::new("fzf");
    fzf.set_mute(true).with_input(input);

    let mut result = fzf.execute()?;
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

pub fn ensure_no_uncommitted() -> Result<()> {
    let mut git = Cmd::git(&["status", "-s"]);
    let lines = git.execute()?.checked_lines()?;
    if !lines.is_empty() {
        bail!(
            "You have {} to handle",
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
    const BRANCH_REGEX: &str = r"^(\*)*[ ]*([^ ]*)[ ]*([^ ]*)[ ]*(\[[^\]]*\])*[ ]*(.*)$";
    const HEAD_BRANCH_PREFIX: &str = "HEAD branch:";

    pub fn get_regex() -> Regex {
        Regex::new(Self::BRANCH_REGEX).expect("parse git branch regex")
    }

    pub fn list() -> Result<Vec<GitBranch>> {
        let re = Self::get_regex();
        let lines = Cmd::git(&["branch", "-vv"])
            .with_desc("List git branch info")
            .execute()?
            .checked_lines()?;
        let mut branches: Vec<GitBranch> = Vec::with_capacity(lines.len());
        for line in lines {
            let branch = Self::parse(&re, line)?;
            branches.push(branch);
        }

        Ok(branches)
    }

    pub fn list_remote(remote: &str) -> Result<Vec<String>> {
        let lines = Cmd::git(&["branch", "-al"])
            .with_desc("List remote git branch")
            .execute()?
            .checked_lines()?;
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
            .with_desc("List local git branch")
            .execute()?
            .checked_lines()?;
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

        let mut git = Cmd::git(&["symbolic-ref", &head_ref]);
        if let Ok(out) = git.execute()?.checked_read() {
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
        let mut git = Cmd::git(&["remote", "show", remote]);
        let lines = git.execute()?.checked_lines()?;
        Self::parse_default_branch(lines)
    }

    pub fn parse_default_branch(lines: Vec<String>) -> Result<String> {
        for line in lines {
            if let Some(branch) = line.trim().strip_prefix(Self::HEAD_BRANCH_PREFIX) {
                let branch = branch.trim();
                if branch.is_empty() {
                    bail!("Default branch returned by git remote show is empty");
                }
                return Ok(branch.to_string());
            }
        }

        bail!("No default branch returned by git remote show, please check your git command");
    }

    pub fn current() -> Result<String> {
        Cmd::git(&["branch", "--show-current"])
            .with_desc("Get current branch info")
            .execute()?
            .checked_read()
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
            .with_desc("List git remotes")
            .execute()?
            .checked_lines()?;
        Ok(lines.iter().map(|s| GitRemote(s.to_string())).collect())
    }

    pub fn new() -> GitRemote {
        GitRemote(String::from("origin"))
    }

    pub fn from_upstream(
        remote: &Remote,
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

        info!("Get upstream for {}", repo.full_name());
        let api_repo = provider.get_repo(&repo.owner, &repo.name)?;
        if let None = api_repo.upstream {
            bail!(
                "Repo {} is not forked, so it has not an upstream",
                repo.full_name()
            );
        }
        let api_upstream = api_repo.upstream.unwrap();
        let upstream = Repo::from_upstream(&remote.name, api_upstream);
        let url = upstream.clone_url(&remote);

        confirm!(
            "Do you want to set upstream to {}: {}",
            upstream.long_name(),
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
        Cmd::git(&["fetch", self.0.as_str(), branch.as_str()])
            .execute()?
            .check()?;
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
        .with_desc(format!("Get commits between {target}"))
        .execute()?
        .checked_lines()?;
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
            .with_desc(format!("Get url for remote {}", self.0))
            .execute()?
            .checked_read()?;
        Ok(url)
    }
}

pub struct GitTag(String);

impl std::fmt::Display for GitTag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl GitTag {
    const NUM_REGEX: &str = r"\d+";
    const PLACEHOLDER_REGEX: &str = r"\{(\d+|%[yYmMdD])(\+)*}";

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    pub fn list() -> Result<Vec<GitTag>> {
        let tags: Vec<_> = Cmd::git(&["tag"])
            .with_desc("Get git tags")
            .execute()?
            .checked_lines()?
            .iter()
            .filter(|line| !line.trim().is_empty())
            .map(|line| GitTag(line.trim().to_string()))
            .collect();
        Ok(tags)
    }

    pub fn latest() -> Result<GitTag> {
        Cmd::git(&["fetch", "origin", "--prune-tags"])
            .with_desc("Fetch tags")
            .execute()?
            .check()?;
        let output = Cmd::git(&["describe", "--tags", "--abbrev=0"])
            .with_desc("Get latest tag")
            .execute()?
            .checked_read()?;
        if output.is_empty() {
            bail!("No latest tag");
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
    steps: &'static Vec<WorkflowStep>,
}

impl Workflow {
    pub fn new(name: impl AsRef<str>) -> Result<Workflow> {
        match config::base().workflows.get(name.as_ref()) {
            Some(steps) => Ok(Workflow {
                name: name.as_ref().to_string(),
                steps,
            }),
            None => bail!("Could not find workeflow {}", name.as_ref()),
        }
    }

    pub fn execute_repo(&self, repo: &Rc<Repo>) -> Result<()> {
        info!("Execute workflow {} for {}", self.name, repo.full_name());
        let dir = repo.get_path();
        for step in self.steps.iter() {
            if let Some(run) = &step.run {
                let script = run.replace("\n", ";");
                let mut cmd = Cmd::sh(&script);
                cmd.with_path(&dir);

                cmd.with_env("REPO_NAME", repo.name.as_str());
                cmd.with_env("REPO_OWNER", repo.owner.as_str());
                cmd.with_env("REMOTE", repo.remote.as_str());
                cmd.with_env("REPO_LONG", repo.long_name());
                cmd.with_env("REPO_FULL", repo.full_name());

                cmd.with_desc(format!("Run {}", step.name));

                cmd.execute()?.check()?;
                continue;
            }
            if let None = step.file {
                continue;
            }

            let content = step.file.as_ref().unwrap();
            let content = content.replace("\\t", "\t");

            exec!("Create file {}", step.name);
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

                cmd.set_mute(true).piped_stderr();

                cmd.execute()?.check()?;
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
            panic!("Empty row");
        }
        if self.ncol == 0 {
            self.ncol = row.len();
        } else if row.len() != self.ncol {
            panic!("Unexpect row len");
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
    const SPACE: &str = " ";
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
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let size = self.upstream.write(buf)?;
        self.current += size;

        if self.current >= self.total {
            self.current = self.total;
            cursor_up_stderr();
            write_stderr(self.render());
            return Ok(size);
        }

        let now = Instant::now();
        let delta = now - self.last_report;
        if delta >= Self::REPORT_INTERVAL {
            cursor_up_stderr();
            write_stderr(self.render());
            self.last_report = now;
        }

        Ok(size)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.upstream.flush()
    }
}

const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(10);

pub fn download(name: &str, url: impl AsRef<str>, path: impl AsRef<str>) -> Result<()> {
    let client = Client::builder().timeout(DOWNLOAD_TIMEOUT).build().unwrap();
    let url = Url::parse(url.as_ref()).context("Parse download url")?;

    let req = client
        .request(Method::GET, url)
        .build()
        .context("Build download http request")?;

    let mut resp = client.execute(req).context("Request http download")?;
    let total = match resp.content_length() {
        Some(size) => size,
        None => bail!("Could not find content-length in http response"),
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
    resp.copy_to(&mut pw).context("Download data")?;

    cursor_up_stderr();
    info!("Download {} done", name);

    Ok(())
}

#[cfg(test)]
mod tests {
    use serial_test::serial;

    use super::*;

    #[test]
    #[serial]
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
    #[serial]
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
    #[serial]
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
    #[serial]
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

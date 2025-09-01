pub mod bash;
pub mod fzf;
pub mod git;

use std::borrow::Cow;
use std::ffi::OsStr;
use std::fmt::{self, Display, Formatter};
use std::io::{self, Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::OnceLock;

use anyhow::{Context, Result, bail};
use regex::Regex;

use crate::term::output;
use crate::{debug, info};

pub struct Cmd {
    cmd: Command,
    input: Option<String>,

    hint: CmdHint,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CmdResult<'a> {
    pub name: &'a str,

    pub code: i32,

    pub stdout: Option<String>,
    pub stderr: Option<String>,
}

/// Custom error type for early exit.
#[derive(Debug)]
pub struct SilentExit {
    pub code: u8,
}

impl Display for SilentExit {
    fn fmt(&self, _: &mut Formatter<'_>) -> fmt::Result {
        Ok(())
    }
}

impl<'a> CmdResult<'a> {
    pub fn check(&self) -> Result<()> {
        if self.code == 0 {
            return Ok(());
        }
        match self.stderr {
            Some(ref stderr) => {
                bail!(
                    "command '{}' exited with bad code {}: {stderr:?}",
                    self.name,
                    self.code
                );
            }
            // The command has already been output to the terminal, and its output
            // has been redirected. No need to print any error messages here.
            None => bail!(SilentExit { code: 130 }),
        }
    }
}

#[derive(Debug, Clone, Default)]
enum CmdHint {
    #[default]
    Raw,
    Message(String),
    None,
}

static STANDARD_ARG_RE: OnceLock<Regex> = OnceLock::new();

fn is_arg_standard(arg: &str) -> bool {
    STANDARD_ARG_RE
        .get_or_init(|| Regex::new(r"^[a-zA-Z0-9._/-]+$").unwrap())
        .is_match(arg)
}

fn get_escaped_arg<'a>(arg: &'a str) -> Cow<'a, str> {
    if is_arg_standard(arg) {
        Cow::Borrowed(arg)
    } else {
        Cow::Owned(format!("{arg:?}"))
    }
}

impl Cmd {
    pub fn new(program: &str) -> Self {
        Self {
            cmd: Command::new(program),
            input: None,
            hint: CmdHint::Raw,
        }
    }

    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.cmd.args(args);
        self
    }

    pub fn mute(mut self) -> Self {
        self.hint = CmdHint::None;
        self
    }

    pub fn message(mut self, message: impl ToString) -> Self {
        self.hint = CmdHint::Message(message.to_string());
        self
    }

    pub fn input(mut self, input: impl ToString) -> Self {
        self.input = Some(input.to_string());
        self
    }

    pub fn current_dir(mut self, dir: impl AsRef<Path>) -> Self {
        self.cmd.current_dir(dir);
        self
    }

    pub fn env<K, V>(mut self, key: K, value: V) -> Self
    where
        K: AsRef<str>,
        V: AsRef<str>,
    {
        self.cmd.env(key.as_ref(), value.as_ref());
        self
    }

    pub fn output(&mut self) -> Result<String> {
        let result = self.execute_unchecked(true)?;
        result.check()?;
        let output = result.stdout.unwrap_or_default();
        Ok(output.trim().to_string())
    }

    pub fn lines(&mut self) -> Result<Vec<String>> {
        let output = self.output()?;
        let lines: Vec<String> = output
            .split('\n')
            .filter_map(|line| {
                if line.is_empty() {
                    return None;
                }
                Some(line.trim().to_string())
            })
            .collect();
        Ok(lines)
    }

    #[inline]
    pub fn execute(&mut self) -> Result<()> {
        self.execute_unchecked(false)?.check()
    }

    pub fn execute_unchecked<'a>(&'a mut self, capture_output: bool) -> Result<CmdResult<'a>> {
        let mut full = None;
        if output::get_debug().is_some() {
            full = Some(self.full());
            debug!(
                "[exec] Begin to execute command: {}",
                full.as_ref().unwrap()
            );
        }

        self.cmd.stdout(io::stderr());
        match self.hint {
            CmdHint::Raw => {
                if full.is_none() {
                    full = Some(self.full());
                }
                info!("Execute: {}", full.unwrap());
            }
            CmdHint::Message(ref message) => info!("{message}"),
            CmdHint::None => {
                self.cmd.stderr(Stdio::piped());
                self.cmd.stdout(Stdio::piped());
                self.cmd.stdin(Stdio::null());
            }
        }
        if capture_output {
            self.cmd.stdout(Stdio::piped());
        }
        if self.input.is_some() {
            self.cmd.stdin(Stdio::piped());
        }

        let mut child = match self.cmd.spawn() {
            Ok(child) => child,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                bail!(
                    "could not find command '{}', please install it first",
                    self.name()
                );
            }
            Err(e) => {
                return Err(e)
                    .with_context(|| format!("could not launch command '{}'", self.name_str()));
            }
        };

        if let Some(ref input) = self.input {
            debug!("[exec] Write intput to command: {input:?}");
            let handle = child.stdin.as_mut().unwrap();
            if let Err(e) = write!(handle, "{input}") {
                return Err(e)
                    .with_context(|| format!("write input to command '{}'", self.name_str()));
            }

            drop(child.stdin.take());
        }

        let mut stdout = child.stdout.take();
        let mut stderr = child.stderr.take();

        let stdout = match stdout {
            Some(ref mut stdout) => {
                let mut buf = String::new();
                stdout
                    .read_to_string(&mut buf)
                    .with_context(|| format!("read stdout of command '{}'", self.name_str()))?;
                Some(buf)
            }
            None => None,
        };
        let stderr = match stderr {
            Some(ref mut stderr) => {
                let mut buf = String::new();
                stderr
                    .read_to_string(&mut buf)
                    .with_context(|| format!("read stderr of command '{}'", self.name_str()))?;
                Some(buf)
            }
            None => None,
        };

        let status = child
            .wait()
            .with_context(|| format!("wait for command '{}' done", self.name_str()))?;
        let result = CmdResult {
            name: self.name(),
            code: status.code().unwrap_or(-1),
            stdout,
            stderr,
        };
        debug!("[exec] Result: {result:?}");
        Ok(result)
    }

    #[inline]
    fn name(&self) -> &str {
        self.cmd.get_program().to_str().unwrap_or("<unknown>")
    }

    #[inline]
    fn name_str(&self) -> String {
        self.name().to_string()
    }

    fn full(&self) -> String {
        let mut raw = Vec::with_capacity(1);
        raw.push(Cow::Borrowed(self.name()));
        for arg in self.cmd.get_args() {
            if let Some(arg) = arg.to_str() {
                raw.push(get_escaped_arg(arg));
            }
        }
        raw.join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execute() {
        let test_cases = [
            (
                Cmd::new("echo").args(["hello"]).mute(),
                false,
                CmdResult {
                    name: "echo",
                    code: 0,
                    stdout: Some("hello\n".to_string()),
                    stderr: Some(String::new()),
                },
            ),
            (
                Cmd::new("echo").args(["hello"]),
                true,
                CmdResult {
                    name: "echo",
                    code: 0,
                    stdout: Some("hello\n".to_string()),
                    stderr: None,
                },
            ),
            (
                Cmd::new("echo").args(["-n", ""]),
                false,
                CmdResult {
                    name: "echo",
                    code: 0,
                    stdout: None,
                    stderr: None,
                },
            ),
            (
                Cmd::new("echo").args(["-n", ""]),
                true,
                CmdResult {
                    name: "echo",
                    code: 0,
                    stdout: Some(String::new()),
                    stderr: None,
                },
            ),
            (
                Cmd::new("sh").args(["-c", "echo 'hello' >&2"]).mute(),
                false,
                CmdResult {
                    name: "sh",
                    code: 0,
                    stdout: Some(String::new()),
                    stderr: Some("hello\n".to_string()),
                },
            ),
            (
                Cmd::new("sh")
                    .args(["-c", "echo 'hello, stdout'; echo 'hello, stderr' >&2"])
                    .mute(),
                false,
                CmdResult {
                    name: "sh",
                    code: 0,
                    stdout: Some("hello, stdout\n".to_string()),
                    stderr: Some("hello, stderr\n".to_string()),
                },
            ),
            (
                Cmd::new("sh")
                    .args(["-c", "echo 'hello, stdout'; echo 'hello, stderr' >&2"])
                    .mute(),
                true,
                CmdResult {
                    name: "sh",
                    code: 0,
                    stdout: Some("hello, stdout\n".to_string()),
                    stderr: Some("hello, stderr\n".to_string()),
                },
            ),
            (
                Cmd::new("sh")
                    .args(["-c", "echo $TEST_ENV"])
                    .env("TEST_ENV", "test_value"),
                true,
                CmdResult {
                    name: "sh",
                    code: 0,
                    stdout: Some("test_value\n".to_string()),
                    stderr: None,
                },
            ),
            (
                Cmd::new("sh")
                    .args(["-c", "echo 'error message' >&2; exit 133"])
                    .mute(),
                false,
                CmdResult {
                    name: "sh",
                    code: 133,
                    stdout: Some(String::new()),
                    stderr: Some("error message\n".to_string()),
                },
            ),
            (
                Cmd::new("sh")
                    .args(["-c", "echo 'stdout message'; exit 1"])
                    .mute(),
                false,
                CmdResult {
                    name: "sh",
                    code: 1,
                    stdout: Some("stdout message\n".to_string()),
                    stderr: Some(String::new()),
                },
            ),
            (
                Cmd::new("cat").input("input data"),
                true,
                CmdResult {
                    name: "cat",
                    code: 0,
                    stdout: Some("input data".to_string()),
                    stderr: None,
                },
            ),
        ];

        for (mut cmd, capture_output, expect) in test_cases {
            let result = cmd.execute_unchecked(capture_output).unwrap();
            assert_eq!(result, expect);
        }
    }

    #[test]
    fn test_not_found() {
        let mut cmd = Cmd::new("not-exist-command").args(["arg1", "arg2"]);
        let err = cmd.execute_unchecked(false).unwrap_err();
        assert_eq!(
            err.to_string(),
            "could not find command 'not-exist-command', please install it first"
        );
    }

    #[test]
    fn test_output() {
        let test_cases = [
            (Cmd::new("echo").args(["hello"]), "hello"),
            (Cmd::new("echo").args([" hello  "]), "hello"),
            (Cmd::new("echo").args(["-n", "  "]), ""),
            (Cmd::new("echo").args(["-n", ""]), ""),
        ];
        for (mut cmd, expect) in test_cases {
            let output = cmd.output().unwrap();
            assert_eq!(output, expect);
        }
    }

    #[test]
    fn test_lines() {
        let test_cases = [
            (
                Cmd::new("echo").args(["hello\nworld"]),
                vec!["hello", "world"],
            ),
            (
                Cmd::new("echo").args(["hello\n\nworld"]),
                vec!["hello", "world"],
            ),
            (
                Cmd::new("echo").args(["  hello  \n  world  "]),
                vec!["hello", "world"],
            ),
            (
                Cmd::new("echo").args(["  hello  \n\n  world  "]),
                vec!["hello", "world"],
            ),
            (Cmd::new("echo").args(["   "]), Vec::<&str>::new()),
            (Cmd::new("echo").args([""]), Vec::<&str>::new()),
        ];
        for (mut cmd, expect) in test_cases {
            let lines = cmd.lines().unwrap();
            assert_eq!(lines, expect);
        }
    }

    #[test]
    fn test_full() {
        let test_cases = [
            (
                Cmd::new("echo").args(["hello", "world"]),
                "echo hello world",
            ),
            (
                Cmd::new("echo").args(["hello world", "foo'bar"]),
                r#"echo "hello world" "foo'bar""#,
            ),
            (
                Cmd::new("my-cmd").args(["arg1", "arg with space", "arg\"with\"quote"]),
                r#"my-cmd arg1 "arg with space" "arg\"with\"quote""#,
            ),
            (
                Cmd::new("git").args(["clone", "https://github.com/fioncat/roxide"]),
                r#"git clone "https://github.com/fioncat/roxide""#,
            ),
            (
                Cmd::new("sh").args(["-c", "echo ${TEST_ENV}; echo \"hello world\""]),
                r#"sh -c "echo ${TEST_ENV}; echo \"hello world\"""#,
            ),
        ];
        for (cmd, expect) in test_cases {
            assert_eq!(cmd.full(), expect);
        }
    }
}

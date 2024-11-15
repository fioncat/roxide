use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};
use console::style;

use crate::errors::SilentExit;
use crate::info;

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
        if self.display.is_none() {
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

    fn trim_output(s: &str, no_break: bool) -> String {
        let s = s.trim();
        if no_break {
            s.replace('\n', "; ").replace('\'', "")
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
pub struct Cmd {
    cmd: Command,
    input: Option<String>,

    display: CmdDisplay,
}

enum CmdDisplay {
    Cmd,
    Hint(String),
    Docker(String, String),
    Script(String),
    None,
}

impl Cmd {
    /// Use `program` to create a new [`Cmd`].
    pub fn new(program: impl AsRef<str>) -> Cmd {
        Self::with_args(program, &[])
    }

    /// Create a new [`Cmd`], with args.
    pub fn with_args<S: AsRef<str>>(program: S, args: &[&str]) -> Cmd {
        let mut cmd = Command::new(program.as_ref());
        if !args.is_empty() {
            cmd.args(args);
        }

        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        Cmd {
            cmd,
            input: None,
            display: CmdDisplay::None,
        }
    }

    /// Create a new git [`Cmd`].
    pub fn git(args: &[&str]) -> Cmd {
        Self::with_args("git", args)
    }

    /// When executing a command, display the command name, args, and a prompt.
    /// If this function is called, the command's stderr will be redirected to the
    /// terminal, and if it fails during execution, it will return a [`SilentExit`].
    pub fn with_display(mut self, hint: impl ToString) -> Self {
        self.set_display(CmdDisplay::Hint(hint.to_string()));
        self
    }

    /// Similar to [`Cmd::with_display`], but when executed, only display the
    /// command without showing prompt information.
    pub fn with_display_cmd(mut self) -> Self {
        self.set_display(CmdDisplay::Cmd);
        self
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
    pub fn sh(script: impl AsRef<str>, display: bool) -> Cmd {
        let mut cmd = Self::with_args("sh", &["-c", script.as_ref()]);
        if display {
            cmd.set_display(CmdDisplay::Script(script.as_ref().to_string()));
        }
        cmd
    }

    pub fn display_docker(&mut self, image: String, script: String) {
        self.set_display(CmdDisplay::Docker(image, script))
    }

    fn set_display(&mut self, display: CmdDisplay) {
        if let CmdDisplay::None = self.display {
            self.display = display;
            // We redirect command's stderr to program's. So that user can view
            // command's error output directly.
            self.cmd.stderr(Stdio::inherit());
        }
    }

    /// Execute the command and return the output as multiple lines.
    /// See: [`CmdResult::lines`].
    pub fn lines(&mut self) -> Result<Vec<String>> {
        self.execute_unchecked()?.lines()
    }

    /// Execute the command and return the output as string. See: [`CmdResult::read`].
    pub fn read(&mut self) -> Result<String> {
        self.execute_unchecked()?.read()
    }

    /// Execute the command and check result.
    pub fn execute(&mut self) -> Result<()> {
        match self.display {
            CmdDisplay::None => {}
            _ => {
                // In this scenario, when the user does not want to capture the
                // stdout output into the program, we need to redirect stdout to
                // the parent process's stderr to prevent the loss of stdout
                // information. The reason for not inheriting is that some
                // functionalities may depend on the parent process's stdout, and we
                // cannot allow the stdout of the subprocess to interfere with these
                // functionalities.
                self.cmd.stdout(io::stderr());
            }
        }
        self.execute_unchecked()?.check()
    }

    /// Execute the command without performing result validation.
    ///
    /// See: [`CmdResult`].
    pub fn execute_unchecked(&mut self) -> Result<CmdResult> {
        let result_display = self.show();

        let mut child = match self.cmd.spawn() {
            Ok(child) => child,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                bail!(
                    "could not find command `{}`, please make sure it is installed",
                    self.get_name()
                );
            }
            Err(e) => {
                return Err(e)
                    .with_context(|| format!("could not launch command `{}`", self.get_name()))
            }
        };

        if let Some(input) = &self.input {
            let handle = child.stdin.as_mut().unwrap();
            if let Err(err) = write!(handle, "{}", input) {
                return Err(err)
                    .with_context(|| format!("write input to command `{}`", self.get_name()));
            }

            drop(child.stdin.take());
        }

        let mut stdout = child.stdout.take();
        let mut stderr = child.stderr.take();

        let stdout = match stdout.as_mut() {
            Some(stdout) => {
                let mut out = String::new();
                stdout
                    .read_to_string(&mut out)
                    .with_context(|| format!("read stdout from command `{}`", self.get_name()))?;
                out
            }
            None => String::new(),
        };
        let stderr = match stderr.as_mut() {
            Some(stderr) => {
                let mut out = String::new();
                stderr
                    .read_to_string(&mut out)
                    .with_context(|| format!("read stderr from command `{}`", self.get_name()))?;
                out
            }
            None => String::new(),
        };

        let status = child.wait().context("Wait command done")?;
        Ok(CmdResult {
            code: status.code(),
            display: result_display,
            stdout,
            stderr,
        })
    }

    fn show(&self) -> Option<String> {
        match &self.display {
            CmdDisplay::None => Some(self.full()),
            CmdDisplay::Cmd => {
                self.show_cmd(self.full());
                None
            }
            CmdDisplay::Hint(hint) => {
                info!("{}", hint);
                self.show_cmd(self.full());
                None
            }
            CmdDisplay::Script(script) => {
                self.show_script(script);
                None
            }
            CmdDisplay::Docker(image, script) => {
                info!("Use docker image {}", style(image).bold());
                self.show_script(script);
                None
            }
        }
    }

    #[inline]
    fn show_script(&self, s: impl AsRef<str>) {
        let script = format!(">> {}", style(s.as_ref()).bold());
        self.show_cmd(script);
    }

    #[inline]
    fn show_cmd(&self, s: impl AsRef<str>) {
        eprintln!("{} {}", style("::").cyan().bold(), s.as_ref());
    }

    #[inline]
    fn full(&self) -> String {
        let mut cmd_args = Vec::with_capacity(1);
        cmd_args.push(self.get_name());
        let args = self.cmd.get_args();
        for arg in args {
            cmd_args.push(arg.to_str().unwrap());
        }
        cmd_args.join(" ")
    }

    #[inline]
    fn get_name(&self) -> &str {
        self.cmd.get_program().to_str().unwrap_or("<unknown>")
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
        let prefix = if !path.is_empty() {
            vec!["-C", path]
        } else {
            vec![]
        };
        GitCmd { prefix }
    }

    /// See: [`Cmd::execute`].
    pub fn exec(&self, args: &[&str]) -> Result<()> {
        let args = [&self.prefix, args].concat();
        Cmd::git(&args).execute()
    }

    /// See: [`Cmd::lines`].
    pub fn lines(&self, args: &[&str]) -> Result<Vec<String>> {
        let args = [&self.prefix, args].concat();
        Cmd::git(&args).lines()
    }

    /// See: [`Cmd::read`].
    pub fn read(&self, args: &[&str]) -> Result<String> {
        let args = [&self.prefix, args].concat();
        Cmd::git(&args).read()
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
pub fn fzf_search<S>(keys: &[S]) -> Result<usize>
where
    S: AsRef<str>,
{
    let mut input = String::with_capacity(keys.len());
    for key in keys {
        input.push_str(key.as_ref());
        input.push('\n');
    }

    let mut fzf = Cmd::new("fzf");
    fzf.with_input(input);

    let result = fzf.execute_unchecked()?;
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

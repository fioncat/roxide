#![allow(dead_code)]

use std::io::{self, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::{env, fs};

use anyhow::{bail, Context, Result};
use console::{style, Term};
use dialoguer::theme::ColorfulTheme;
use dialoguer::Input;
use serde::Serialize;
use serde_json::ser::PrettyFormatter;
use serde_json::Serializer;

use crate::config::Config;
use crate::errors::SilentExit;
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
    stderr!("{}", json);
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
                print!("{head}{item}");
            } else {
                print!("{head_space}{item}");
            }
            current_size = head_size + item_size;
            continue;
        }

        current_size += 2 + item_size;
        if current_size > col_size {
            stderr!();
            stderr!("{}{}", head_space, item);
            current_size = head_size + item_size;
            continue;
        }

        print!("  {item}");
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

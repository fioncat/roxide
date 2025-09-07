use std::sync::OnceLock;

use anyhow::{Result, bail};

use crate::config::CmdConfig;
use crate::debug;

use super::{Cmd, SilentExit};

static FZF_COMMAND_CONFIG: OnceLock<CmdConfig> = OnceLock::new();

pub fn set_cmd(cfg: CmdConfig) {
    let _ = FZF_COMMAND_CONFIG.set(cfg);
}

pub fn get_cmd() -> Cmd {
    FZF_COMMAND_CONFIG
        .get()
        .map(|cfg| Cmd::new(&cfg.name).args(&cfg.args))
        .unwrap_or(Cmd::new("fzf"))
}

pub fn search<S>(desc: &str, items: &[S], filter: Option<&str>) -> Result<usize>
where
    S: AsRef<str> + std::fmt::Debug,
{
    let mut cmd = get_cmd();
    if let Some(f) = filter {
        cmd = cmd.args(["--filter", f]);
    }
    debug!("[fzf] Begin to run fzf search for {items:?}");

    let mut input = String::new();
    for item in items {
        input.push_str(item.as_ref());
        input.push('\n');
    }

    cmd = cmd.message(desc).input(input);

    let mut result = cmd.execute_unchecked(true)?;
    let stdout = result.stdout.take().unwrap_or_default();
    match result.code {
        0 => {
            let output = stdout.trim();
            if output.is_empty() {
                bail!("fzf returned empty output");
            }
            debug!("[fzf] Result: {output:?}");
            match items.iter().position(|item| item.as_ref() == output) {
                Some(index) => Ok(index),
                None => bail!("fzf returned unknown output: {output:?}"),
            }
        }
        1 => bail!("fzf no match found"),
        2 => bail!("fzf returned error"),
        130 => bail!(SilentExit { code: 130 }),
        128..=254 => bail!("fzf terminated"),
        _ => bail!("fzf unknown exit code: {}", result.code),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fzf() {
        let test_cases = [
            (vec!["apple", "banana", "cherry"], "banna", Some(1)),
            (vec!["dog", "cat", "mouse"], "elephant", None),
            (vec![], "anything", None),
        ];
        for (items, filter, expected) in test_cases {
            let result = search("Test search", &items, Some(filter));
            match expected {
                Some(index) => {
                    assert!(result.is_ok());
                    assert_eq!(result.unwrap(), index);
                }
                None => {
                    assert!(result.is_err());
                    assert_eq!(result.unwrap_err().to_string(), "fzf no match found");
                }
            }
        }
    }
}

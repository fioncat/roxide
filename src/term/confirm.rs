use std::sync::OnceLock;

use anyhow::{Context, Result, bail};
use console::style;
use scanf::scanf;

use crate::exec::SilentExit;
use crate::{debug, info, output, outputln};

static NO_CONFIRM: OnceLock<bool> = OnceLock::new();

pub fn set_no_confirm(no_confirm: bool) {
    let _ = NO_CONFIRM.set(no_confirm);
}

fn is_no_confirm() -> bool {
    NO_CONFIRM.get().copied().unwrap_or_default()
}

#[macro_export]
macro_rules! confirm {
    ($($arg:tt)*) => {
        if !cfg!(test) {
            $crate::term::confirm::confirm(format!($($arg)*))?;
        }
    };
}

pub fn confirm(msg: String) -> Result<()> {
    if is_no_confirm() {
        return Ok(());
    }

    debug!("[confirm] Wait user confirm");
    let msg = format!(":: {msg}");
    let styled = style(msg).bold();
    output!("{styled}? [Y/n] ");

    let mut resp = String::new();
    scanf!("{resp}").context("confirm: scan terminal stdin failed")?;
    debug!("[confirm] User input: {resp:?}");
    let resp = resp.trim().to_lowercase();
    if resp != "y" {
        bail!(SilentExit { code: 130 });
    }

    Ok(())
}

pub fn confirm_items<T, S>(
    items: T,
    action: &str,
    noun: &str,
    name: &str,
    plural: &str,
) -> Result<()>
where
    T: AsRef<[S]>,
    S: AsRef<str>,
{
    if is_no_confirm() {
        return Ok(());
    }

    if cfg!(test) {
        return Ok(());
    }

    let items = items.as_ref();
    if items.is_empty() {
        bail!("nothing to {action}");
    }

    info!("Require confirm to {action}");
    outputln!();

    let width = super::width();

    let name = if items.len() == 1 { name } else { plural };
    let head = format!("{name} ({}): ", items.len());
    let head_width = console::measure_text_width(head.as_str());
    let head_space = " ".repeat(head_width);

    let mut current_width: usize = 0;
    for (idx, item) in items.iter().enumerate() {
        let item = item.as_ref();
        let item_size = console::measure_text_width(item);
        if current_width == 0 {
            if idx == 0 {
                output!("{head}{item}");
            } else {
                output!("{head_space}{item}");
            }
            current_width = head_width + item_size;
            continue;
        }

        current_width += 2 + item_size;
        if current_width > width {
            outputln!();
            output!("{head_space}{item}");
            current_width = head_width + item_size;
            continue;
        }

        output!("  {item}");
    }
    outputln!("\n");

    outputln!("Total {} {} to {action}", items.len(), name.to_lowercase());
    confirm!("Proceed with {noun}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_confirm() -> Result<i32> {
        confirm!("Do you want to continue");
        set_no_confirm(true);
        confirm!("Always ok");
        set_no_confirm(false);
        confirm!("Always ok");
        Ok(12)
    }

    #[test]
    fn test_confirm() {
        assert_eq!(run_confirm().unwrap(), 12);
    }
}

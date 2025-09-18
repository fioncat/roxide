pub mod funcs;

use std::collections::HashSet;
use std::env;
use std::fmt::Display;

use anyhow::{Context, Result, bail};

use crate::config::context::ConfigContext;
use crate::debug;

use super::{App, Command};

#[derive(Debug, Clone, Default)]
pub struct CompleteCommand {
    pub name: &'static str,
    pub alias: Option<Vec<&'static str>>,
    pub subcommands: Option<Vec<CompleteCommand>>,
    pub args: Option<Vec<CompleteArg>>,
}

pub struct CompleteContext {
    pub ctx: ConfigContext,
    pub current: String,
    pub args: Vec<String>,
}

impl Display for CompleteContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "args={:?}, current={:?}", self.args, self.current)
    }
}

#[derive(Debug, Clone, Default)]
struct CompleteResult {
    pub items: Vec<String>,
    pub files: bool,
    pub dirs: bool,
}

type CompleteFunc = fn(CompleteContext) -> Result<Vec<String>>;

#[derive(Debug, Clone, Default)]
pub struct CompleteArg {
    long: Option<&'static str>,
    short: Option<char>,
    complete_func: Option<CompleteFunc>,
    array: bool,
    files: bool,
    dirs: bool,
}

impl CompleteCommand {
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            ..Default::default()
        }
    }

    pub fn alias(mut self, alias: &'static str) -> Self {
        if let Some(ref mut a) = self.alias {
            a.push(alias);
        } else {
            self.alias = Some(vec![alias]);
        }
        self
    }

    pub fn subcommand(mut self, cmd: CompleteCommand) -> Self {
        if let Some(ref mut cmds) = self.subcommands {
            cmds.push(cmd);
        } else {
            self.subcommands = Some(vec![cmd]);
        }
        self
    }

    pub fn arg(mut self, arg: CompleteArg) -> Self {
        if let Some(ref mut args) = self.args {
            args.push(arg);
        } else {
            self.args = Some(vec![arg]);
        }
        self
    }

    pub fn args<I>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = CompleteArg>,
    {
        if let Some(ref mut a) = self.args {
            a.extend(args);
        } else {
            self.args = Some(args.into_iter().collect());
        }
        self
    }
}

impl CompleteArg {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn files(mut self) -> Self {
        self.files = true;
        self
    }

    pub fn dirs(mut self) -> Self {
        self.dirs = true;
        self
    }

    pub fn long(mut self, long: &'static str) -> Self {
        self.long = Some(long);
        self
    }

    pub fn short(mut self, short: char) -> Self {
        self.short = Some(short);
        self
    }

    pub fn array(mut self) -> Self {
        self.array = true;
        self
    }

    pub fn complete(mut self, func: CompleteFunc) -> Self {
        self.complete_func = Some(func);
        self
    }

    pub fn no_complete_value(mut self) -> Self {
        self.complete_func = Some(funcs::no_complete);
        self
    }

    fn run_complete(&self, cmp_ctx: CompleteContext) -> Result<CompleteResult> {
        if self.files {
            return Ok(CompleteResult::files());
        }
        if self.dirs {
            return Ok(CompleteResult::dirs());
        }
        if let Some(f) = self.complete_func {
            let items = f(cmp_ctx)?;
            return Ok(CompleteResult::items(items));
        }
        Ok(CompleteResult::default())
    }

    fn matched_flag(&self, flag: &str) -> bool {
        if let Some(long) = self.long
            && flag == long
        {
            return true;
        }
        if let Some(short) = self.short
            && flag.len() == 1
            && flag.chars().next().unwrap() == short
        {
            return true;
        }
        false
    }

    fn has_value(&self) -> bool {
        self.dirs || self.files || self.complete_func.is_some()
    }
}

impl CompleteResult {
    pub fn items(items: Vec<String>) -> Self {
        Self {
            items,
            ..Default::default()
        }
    }

    pub fn files() -> Self {
        Self {
            files: true,
            ..Default::default()
        }
    }

    pub fn dirs() -> Self {
        Self {
            dirs: true,
            ..Default::default()
        }
    }
}

const INIT_ENV: &str = "ROXIDE_INIT";
const INDEX_ENV: &str = "ROXIDE_COMPLETE_INDEX";

pub fn register_complete() -> Result<bool> {
    let Ok(shell) = env::var(INIT_ENV) else {
        return Ok(false);
    };
    if shell != "zsh" && shell != "bash" {
        bail!("does not support shell {shell:?}, please use `bash` or `zsh` instead");
    }

    let ctx = ConfigContext::setup()?;

    let mut args = env::args().collect::<Vec<_>>();
    debug!("[complete] Begin to handle completion, args: {args:?}");
    if args.len() == 1 {
        debug!("[complete] No extra args, print init script");
        let binary = env::current_exe().context("failed to get current exe path")?;
        let binary = format!("{}", binary.display());
        let init_script = include_str!("../../../hack/rox.sh").replace("{{binary}}", &binary);
        let complete_script = match shell.as_str() {
            "zsh" => include_str!("../../../hack/zsh_complete.sh"),
            "bash" => include_str!("../../../hack/bash_complete.sh"),
            _ => unreachable!(),
        };

        println!("{init_script}");
        print!("{complete_script}");
        return Ok(true);
    }

    let Ok(index) = env::var(INDEX_ENV) else {
        debug!("[complete] Error: Complete index env not set");
        return Ok(true);
    };

    let index: usize = match index.parse() {
        Ok(i) => i,
        Err(e) => {
            debug!("[complete] Error: Invalid complete index env: {e:#}");
            return Ok(true);
        }
    };
    let current = if index < args.len() {
        args.remove(index)
    } else {
        String::new()
    };
    if !args.is_empty() {
        args.remove(0); // remove binary path
    }

    let result = match complete_command(ctx, args, current, App::complete()) {
        Ok(r) => r,
        Err(e) => {
            debug!("[complete] Failed to complete: {e:#}");
            return Ok(false);
        }
    };

    if result.files {
        println!("0");
    } else if result.dirs {
        println!("1")
    } else {
        println!("2");
    }

    for item in result.items {
        println!("{item}");
    }
    Ok(true)
}

fn complete_command(
    ctx: ConfigContext,
    mut args: Vec<String>,
    current: String,
    cmd: CompleteCommand,
) -> Result<CompleteResult> {
    debug!("[complete] The root command is: {cmd:?}");
    debug!("[complete] Args: {args:?}, current: {current:?}");

    let cmd = get_final_command(cmd, &mut args);
    debug!("[complete] The final command is: {cmd:?}, final args is: {args:?}");

    let Some(cmd) = cmd else {
        debug!("[complete] No such command");
        return Ok(CompleteResult::default());
    };

    if let Some(subcommands) = cmd.subcommands {
        let items = subcommands.iter().map(|c| c.name.to_string()).collect();
        debug!(
            "[complete] Complete subcommands for {:?}: {items:?}",
            cmd.name
        );
        return Ok(CompleteResult::items(items));
    }

    debug!(
        "[complete] No subcommands, try to complete flags or args for {:?}",
        cmd.name
    );
    let Some(mut cmp_args) = cmd.args else {
        debug!("[complete] No args to complete");
        return Ok(CompleteResult::default());
    };

    if current.starts_with('-') {
        return Ok(complete_flag_name(cmp_args, args));
    }

    if let Some(last) = args.last()
        && last.starts_with('-')
    {
        let flag_name = last.trim_start_matches('-');
        let flag_pos = cmp_args
            .iter()
            .position(|a| a.has_value() && a.matched_flag(flag_name));
        if let Some(pos) = flag_pos {
            // The previous arg is a flag that requires a value, complete its value
            let cmp_arg = cmp_args.remove(pos);
            return complete_flag_value(ctx, cmp_arg, current, args);
        };
    }

    complete_arg(ctx, cmp_args, current, args)
}

fn complete_flag_name(cmp_args: Vec<CompleteArg>, args: Vec<String>) -> CompleteResult {
    debug!("[complete] Complete flag name, args: {args:?}");
    let mut exsisting_flags = HashSet::new();
    for cmp_arg in cmp_args.iter() {
        if cmp_arg.array {
            // We allow array flag to be used multiple times, so don't filter it out
            continue;
        }
        if let Some(long) = cmp_arg.long {
            exsisting_flags.insert(format!("--{long}"));
        }
        if let Some(short) = cmp_arg.short {
            exsisting_flags.insert(format!("-{short}"));
        }
    }
    debug!("[complete] Existing flags: {exsisting_flags:?}");

    let mut excludes = HashSet::new();
    for arg in args {
        if exsisting_flags.contains(&arg) {
            excludes.insert(arg);
        }
    }
    debug!("[complete] Exclude flags: {excludes:?}");

    let items: Vec<String> = cmp_args
        .into_iter()
        .filter_map(|a| {
            if let Some(long) = a.long {
                let flag = format!("--{long}");
                if excludes.contains(&flag) {
                    return None;
                }
                return Some(flag);
            }
            if let Some(short) = a.short {
                let flag = format!("-{short}");
                if excludes.contains(&flag) {
                    return None;
                }
                return Some(flag);
            }
            None
        })
        .collect();
    debug!("[complete] Complete flag names: {items:?}");
    CompleteResult::items(items)
}

fn complete_flag_value(
    ctx: ConfigContext,
    cmp_arg: CompleteArg,
    current: String,
    mut args: Vec<String>,
) -> Result<CompleteResult> {
    debug!("[complete] Complete flag value for {cmp_arg:?}, current: {current:?}, args: {args:?}");
    args.pop(); // remove flag

    // To check and collect if we have the same flag value before
    let mut args = args.into_iter();
    let mut values = vec![];
    loop {
        let Some(arg) = args.next() else {
            break;
        };
        if !arg.starts_with('-') {
            continue;
        }
        let flag_name = arg.trim_start_matches('-');
        if !cmp_arg.matched_flag(flag_name) {
            continue;
        }

        // Yes, we have the same flag, if this flag is not array, current completion
        // is invalid, just return empty
        if !cmp_arg.array {
            debug!("[complete] The flag is not array, return empty");
            return Ok(CompleteResult::default());
        }

        // The next arg should be the value for this flag
        let Some(value) = args.next() else {
            // No value, now just ignore it
            continue;
        };

        values.push(value);
    }
    debug!("[complete] The previous values for the flag: {values:?}");
    let result = cmp_arg.run_complete(CompleteContext {
        ctx,
        current,
        args: values,
    })?;
    debug!("[complete] Complete flag result: {result:?}");
    Ok(result)
}

fn complete_arg(
    ctx: ConfigContext,
    cmp_args: Vec<CompleteArg>,
    current: String,
    args: Vec<String>,
) -> Result<CompleteResult> {
    debug!("[complete] Complete arg, current: {current:?}, args: {args:?}");

    // Collect all flags with values
    let mut flags_with_values: HashSet<String> = HashSet::new();
    let mut filtered_cmp_args = vec![];
    for cmp_arg in cmp_args {
        if let Some(long) = cmp_arg.long {
            let flag = format!("--{long}");
            if cmp_arg.has_value() {
                flags_with_values.insert(flag);
            }
        }
        if let Some(short) = cmp_arg.short {
            let flag = format!("-{short}");
            if cmp_arg.has_value() {
                flags_with_values.insert(flag);
            }
        }
        if cmp_arg.long.is_none() && cmp_arg.short.is_none() {
            filtered_cmp_args.push(cmp_arg);
        }
    }
    debug!("[complete] Flags with values: {flags_with_values:?}");

    // Filter out all flags from args
    let mut args = args.into_iter();
    let mut filtered = vec![];
    loop {
        let Some(arg) = args.next() else {
            break;
        };
        if arg.starts_with('-') {
            if flags_with_values.contains(&arg) {
                // This is a flag that requires a value, skip the next arg as well
                args.next();
            }
            continue;
        }
        filtered.push(arg);
    }
    debug!("[complete] Filtered args: {filtered:?}");

    // Locate to the current arg position
    let mut cmp_args = filtered_cmp_args.into_iter();
    for _ in 0..filtered.len() {
        if cmp_args.next().is_none() {
            // The current arg position exceeds the defined args, just return empty
            debug!("[complete] No enough complete args for current completion");
            return Ok(CompleteResult::default());
        }
    }

    let Some(cmp_arg) = cmp_args.next() else {
        debug!("[complete] No complete arg for current completion");
        return Ok(CompleteResult::default());
    };

    debug!("[complete] Found complete arg for current completion: {cmp_arg:?}");
    let result = cmp_arg.run_complete(CompleteContext {
        ctx,
        current,
        args: filtered,
    })?;
    debug!("[complete] Complete arg result: {result:?}");
    Ok(result)
}

fn get_final_command(cmd: CompleteCommand, args: &mut Vec<String>) -> Option<CompleteCommand> {
    if cmd.subcommands.is_none() {
        return Some(cmd);
    }
    if args.is_empty() {
        return Some(cmd);
    }

    let cmd_name = args.remove(0);
    for subcommand in cmd.subcommands.unwrap() {
        if subcommand.name == cmd_name {
            return get_final_command(subcommand, args);
        }
        if let Some(ref alias) = subcommand.alias {
            for a in alias {
                if *a == cmd_name {
                    return get_final_command(subcommand, args);
                }
            }
        }
    }

    None
}

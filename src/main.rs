mod api;
mod batch;
mod cmd;
mod config;
mod errors;
mod repo;
mod secret;
mod term;
mod utils;
mod workflow;

use std::env;
use std::ffi::OsString;
use std::io;
use std::process;

use anyhow::Result;
use clap::error::ErrorKind as ArgsErrorKind;
use clap::Parser;
use nix::unistd;

use crate::cmd::{App, Run};
use crate::config::Config;
use crate::errors::SilentExit;

#[inline(always)]
fn wrap_result<T>(result: Result<T>, message: &str, error_code: i32) -> T {
    match result {
        Ok(value) => value,
        Err(err) => match err.downcast::<SilentExit>() {
            Ok(SilentExit { code }) => process::exit(errors::CODE_SILENT_EXIT + code as i32),
            Err(err) => {
                error!("{} error: {:#}", message, err);
                process::exit(error_code);
            }
        },
    }
}

fn main() {
    let mut args: Vec<OsString> = env::args_os().collect();
    let is_complete = args.get(1).is_some_and(|arg| arg == "complete");

    if !is_complete && !termion::is_tty(&io::stderr()) {
        // We donot allow stderr been redirected, this will cause message been dismissed.
        // Another reason we do this check is that the terminal control characters will be
        // printed in stderr, redirecting it to non-tty will cause confusion.
        // The `complete` command is a special condition, its output is allowed to be
        // dismissed since we donot care about it.
        process::exit(errors::CODE_STDERR_REDIRECT);
    }
    // It is safe to set this since all the colored texts will be printed to stderr.
    console::set_colors_enabled(true);

    // TODO: Support Windows
    if unistd::getuid().is_root() {
        match args.iter().position(|arg| arg == "--allow-root-privieges") {
            Some(pos) => {
                warn!("Launching roxide with root privileges can destory your system, it is strongly not recommanded to do this");
                args.remove(pos);
            }
            None => {
                error!("Launching roxide with root privileges is not allowed (HINT: You can add `--allow-root-privieges` to omit this check)");
                process::exit(errors::CODE_ROOT_PRIVILEGES);
            }
        }
    }

    let app = match App::try_parse_from(args) {
        Ok(app) => app,
        Err(err) => {
            err.use_stderr();
            err.print().expect("write help message to stderr");
            if matches!(
                err.kind(),
                ArgsErrorKind::DisplayHelp
                    | ArgsErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
                    | ArgsErrorKind::DisplayVersion
            ) {
                return;
            }
            eprintln!();
            error!("Parse command line args failed");
            process::exit(errors::CODE_PARSE_COMMAND_LINE_ARGS);
        }
    };

    let cfg = wrap_result(Config::load(), "Load config", errors::CODE_LOAD_CONFIG);
    wrap_result(app.run(&cfg), "Command", errors::CODE_COMMAND_FAILED);
}

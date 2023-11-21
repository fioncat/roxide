mod handlers;

use std::collections::HashMap;

use anyhow::{Error, Result};
use clap::Args;
use strum::VariantNames;

use crate::cmd::complete::handlers::Complete;
use crate::cmd::Run;

use super::app::Commands;

/// Complete support command, please donot use directly.
#[derive(Args)]
pub struct CompleteArgs {
    /// The complete args.
    #[clap(allow_hyphen_values = true)]
    pub args: Vec<String>,
}

macro_rules! build_handlers {
    ($($key:expr => $value:expr), + $(,)?) => {
        {
            let mut map: HashMap<&'static str, fn(&[&str]) -> Result<Complete>> =
                HashMap::new();
            $(
                map.insert($key, $value);
            )+
            map
        }
    };
}

impl CompleteArgs {
    fn handlers() -> HashMap<&'static str, fn(&[&str]) -> Result<Complete>> {
        build_handlers! {
            "attach" => handlers::owner,
            "branch" => handlers::branch,
            "config" => handlers::remote,
            "gc" => handlers::owner,
            "get" => handlers::repo,
            "home" => handlers::repo,
            "import" => handlers::owner,
            "init" => handlers::shell,
            "merge" => handlers::branch,
            "rebase" => handlers::branch,
            "release" => handlers::release,
            "remove" => handlers::repo,
            "reset" => handlers::branch,
            "run" => handlers::run,
            "snapshot" => handlers::snapshot,
            "sync" => handlers::repo,
            "sync-rule" => handlers::sync_rule,
            "tag" => handlers::tag,
            "copy" => handlers::owner,
        }
    }

    fn handle_err(_err: Error) {
        // TODO: implement this, write error log to a file.
    }
}

impl Run for CompleteArgs {
    fn run(&self) -> Result<()> {
        if self.args.is_empty() {
            return Ok(());
        }

        if self.args.len() == 1 {
            let cmds = Commands::VARIANTS;
            let mut cmds: Vec<_> = cmds.into_iter().map(|key| key.to_string()).collect();
            cmds.sort();
            Complete::from(cmds).show();
            return Ok(());
        }

        let handlers = Self::handlers();
        if let Some(complete) = handlers.get(self.args[0].as_str()) {
            let args: Vec<&str> = self
                .args
                .iter()
                .filter(|arg| !arg.starts_with("-"))
                .map(|arg| arg.as_str())
                .collect();
            let args = &args[1..];
            let result = complete(args);
            match result {
                Ok(cmp) => cmp.show(),
                Err(err) => Self::handle_err(err),
            }
        }

        Ok(())
    }
}

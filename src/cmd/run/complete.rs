use std::collections::HashMap;

use anyhow::{Error, Result};
use clap::Args;

use crate::cmd::complete::attach;
use crate::cmd::complete::branch;
use crate::cmd::complete::home;
use crate::cmd::complete::owner;
use crate::cmd::complete::release;
use crate::cmd::complete::remote;
use crate::cmd::complete::run;
use crate::cmd::complete::snapshot;
use crate::cmd::complete::tag;
use crate::cmd::complete::Complete;
use crate::cmd::Run;

/// Complete support command, please donot use directly.
#[derive(Args)]
pub struct CompleteArgs {
    /// The complete args.
    #[clap(allow_hyphen_values = true)]
    pub args: Vec<String>,
}

macro_rules! get_cmds {
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

fn no_complete(_args: &[&str]) -> Result<Complete> {
    Ok(Complete::empty())
}

impl CompleteArgs {
    fn get_cmds() -> HashMap<&'static str, fn(&[&str]) -> Result<Complete>> {
        get_cmds! {
            "home" => home::complete,
            "attach" => attach::complete,
            "branch" => branch::complete,
            "merge" => branch::complete,
            "remove" => home::complete,
            "detach" => no_complete,
            "config" => remote::complete,
            "get" => home::complete,
            "rebase" => branch::complete,
            "squash" => branch::complete,
            "tag" => tag::complete,
            "open" => no_complete,
            "release" => release::complete,
            "reset" => branch::complete,
            "update" => no_complete,
            "clear" => owner::complete,
            "import" => owner::complete,
            "run" => run::complete,
            "sync" => no_complete,
            "snapshot" => snapshot::complete,
            "version" => no_complete,
            "gc" => owner::complete,
            "recover" => no_complete,
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
        let cmds = Self::get_cmds();
        if self.args.len() == 1 {
            let mut keys: Vec<_> = cmds.into_keys().map(|key| key.to_string()).collect();
            keys.sort();
            Complete::from(keys).show();
            return Ok(());
        }

        if let Some(complete) = cmds.get(self.args[0].as_str()) {
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

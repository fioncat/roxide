use std::collections::HashMap;

use anyhow::Error;
use anyhow::Result;

use crate::cmd::app::CompleteArgs;
use crate::cmd::complete::attach;
use crate::cmd::complete::home;
use crate::cmd::complete::Complete;
use crate::cmd::Run;

macro_rules! get_cmds {
    ($($key:expr => $value:expr), + $(,)?) => {
        {
            let mut map: HashMap<&'static str, fn(&[String]) -> Result<Complete>> =
                HashMap::new();
            $(
                map.insert($key, $value);
            )+
            map
        }
    };
}

impl CompleteArgs {
    fn get_cmds() -> HashMap<&'static str, fn(&[String]) -> Result<Complete>> {
        get_cmds! {
            "home" => home::complete,
            "attach" => attach::complete,
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
        // TODO: Handle stash("-x")
        let cmds = Self::get_cmds();
        if self.args.len() == 1 {
            let mut keys: Vec<_> = cmds.into_keys().map(|key| key.to_string()).collect();
            keys.sort();
            Complete::from(keys).show();
            return Ok(());
        }

        if let Some(complete) = cmds.get(self.args[0].as_str()) {
            let args = &self.args[1..];
            let result = complete(args);
            match result {
                Ok(cmp) => cmp.show(),
                Err(err) => Self::handle_err(err),
            }
        }

        Ok(())
    }
}

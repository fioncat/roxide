use anyhow::Result;
use roxide::config;

use crate::cmd::{
    app::{InitArgs, Shell},
    Run,
};

impl Run for InitArgs {
    fn run(&self) -> Result<()> {
        let complete_bytes = match self.shell {
            Shell::Zsh => include_bytes!("../../../scripts/complete_zsh.zsh"),
        };
        println!("{}", String::from_utf8_lossy(complete_bytes));
        println!();

        if let Some(base) = &config::base().command.base {
            let init_bytes = include_bytes!("../../../scripts/init.sh");
            let init_script = String::from_utf8_lossy(init_bytes);
            let script = init_script.replace("_roxide_base", base);
            println!("{script}");
            if let Some(home) = &config::base().command.home {
                println!("alias {home}='{base} home'")
            }
            for (remote, alias) in &config::base().command.remotes {
                println!("alias {alias}='{base} home {remote}'");
            }
            println!();
        }

        Ok(())
    }
}

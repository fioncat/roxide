use std::collections::HashMap;

use anyhow::Result;
use clap::Args;
use strum::VariantNames;

use crate::cmd::{Commands, Completion, CompletionResult, Run};
use crate::config::Config;

/// Completion support command, please don't use directly.
#[derive(Args)]
pub struct CompleteArgs {
    /// The complete args.
    #[clap(allow_hyphen_values = true)]
    pub args: Vec<String>,
}

impl Run for CompleteArgs {
    fn run(&self, cfg: &Config) -> Result<()> {
        let cmds = Commands::VARIANTS;
        let mut cmds: Vec<_> = cmds.iter().map(|key| key.to_string()).collect();
        cmds.sort();

        let comps = Commands::get_completions();

        let result = self.complete(cfg, cmds, comps)?;
        result.show();

        Ok(())
    }
}

impl CompleteArgs {
    fn complete(
        &self,
        cfg: &Config,
        cmds: Vec<String>,
        comps: HashMap<&str, Completion>,
    ) -> Result<CompletionResult> {
        if self.args.is_empty() {
            return Ok(CompletionResult::empty());
        }

        if self.args.len() == 1 {
            return Ok(CompletionResult::from(cmds));
        }
        let name = &self.args[0];

        let completion = match comps.get(name.as_str()) {
            Some(comp) => comp,
            None => return Ok(CompletionResult::empty()),
        };

        let mut args = Vec::with_capacity(self.args.len());

        let mut args_iter = ArgsIter::new(&self.args);
        // Skip the first command name.
        args_iter.next();
        while let Some(arg) = args_iter.next() {
            let arg = arg.to_string();

            if arg.starts_with('-') {
                if completion.flags.is_none() {
                    continue;
                }

                let flag = arg.trim_start_matches('-');
                if flag.is_empty() {
                    continue;
                }

                let flag = match flag.bytes().last() {
                    Some(ch) => ch as char,
                    None => continue,
                };

                let to_complete = match args_iter.view() {
                    Some(arg) => arg.to_string(),
                    None => return Ok(CompletionResult::empty()),
                };

                let result = completion.flags.as_ref().unwrap()(cfg, &flag, &to_complete)?;
                if let Some(result) = result {
                    args_iter.next();
                    if args_iter.view().is_none() {
                        return Ok(result);
                    }
                }

                continue;
            }

            args.push(arg);
        }

        let args: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        (completion.args)(cfg, &args)
    }
}

struct ArgsIter<'a> {
    args: &'a Vec<String>,
    idx: usize,
}

impl ArgsIter<'_> {
    fn new(args: &Vec<String>) -> ArgsIter {
        ArgsIter { args, idx: 0 }
    }

    fn next(&mut self) -> Option<&str> {
        let idx = self.idx;
        self.idx += 1;
        match self.args.get(idx) {
            Some(s) => Some(s.as_str()),
            None => None,
        }
    }

    fn view(&self) -> Option<&str> {
        match self.args.get(self.idx) {
            Some(s) => Some(s.as_str()),
            None => None,
        }
    }
}

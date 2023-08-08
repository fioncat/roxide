use anyhow::Result;

use crate::cmd::complete::home;
use crate::cmd::complete::Complete;
use crate::config;

pub fn complete(args: &[&str]) -> Result<Complete> {
    if args.is_empty() {
        return Ok(Complete::empty());
    }
    if args.len() == 1 {
        let mut names: Vec<String> = config::base()
            .workflows
            .iter()
            .map(|(key, _)| key.clone())
            .collect();
        names.sort();
        return Ok(Complete::from(names));
    }
    home::complete(&args[1..])
}

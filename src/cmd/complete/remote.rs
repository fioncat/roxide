use anyhow::Result;

use crate::cmd::complete::Complete;
use crate::config;

pub fn complete(args: &[&str]) -> Result<Complete> {
    match args.len() {
        0 | 1 => {
            let remotes = config::list_remotes();
            let items: Vec<_> = remotes
                .into_iter()
                .map(|remote| remote.to_string())
                .collect();
            Ok(Complete::from(items))
        }
        _ => Ok(Complete::empty()),
    }
}

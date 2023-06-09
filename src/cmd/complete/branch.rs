use anyhow::Result;
use roxide::shell::GitBranch;

use super::Complete;

pub fn complete(args: &[&str]) -> Result<Complete> {
    match args.len() {
        0 | 1 => {
            let branches = GitBranch::list()?;
            let items: Vec<_> = branches
                .into_iter()
                .filter(|branch| !branch.current)
                .map(|branch| branch.name)
                .collect();
            Ok(Complete::from(items))
        }
        _ => Ok(Complete::empty()),
    }
}

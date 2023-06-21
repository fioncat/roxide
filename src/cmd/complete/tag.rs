use anyhow::Result;

use crate::cmd::complete::Complete;
use crate::shell::GitTag;

pub fn complete(args: &[&str]) -> Result<Complete> {
    match args.len() {
        0 | 1 => {
            let tags = GitTag::list()?;
            let items: Vec<_> = tags.into_iter().map(|tag| tag.to_string()).collect();
            Ok(Complete::from(items))
        }
        _ => Ok(Complete::empty()),
    }
}

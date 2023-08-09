use anyhow::Result;

use crate::cmd::complete::Complete;
use crate::repo::snapshot::Snapshot;

pub fn complete(args: &[&str]) -> Result<Complete> {
    match args.len() {
        0 | 1 => {
            let names = Snapshot::list()?;
            Ok(Complete::from(names))
        }
        _ => Ok(Complete::empty()),
    }
}

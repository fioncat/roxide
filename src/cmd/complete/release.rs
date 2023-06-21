use anyhow::Result;

use crate::{cmd::complete::Complete, config};

pub fn complete(args: &[&str]) -> Result<Complete> {
    match args.len() {
        0 | 1 => {
            let mut items: Vec<_> = config::base()
                .release
                .iter()
                .map(|(key, _val)| key.clone())
                .collect();
            items.sort();
            Ok(Complete::from(items))
        }
        _ => Ok(Complete::empty()),
    }
}

use std::borrow::Cow;
use std::path::Path;

use anyhow::{Result, bail};
use serde::Serialize;

use crate::{
    debug,
    term::list::{List, ListItem},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Tag {
    pub name: String,
    pub commit_id: String,
    pub commit_message: String,
}

pub struct TagList {
    pub tags: Vec<Tag>,
    pub total: u32,
}

impl Tag {
    pub fn list<P>(path: Option<P>, mute: bool) -> Result<Vec<Self>>
    where
        P: AsRef<Path> + std::fmt::Debug,
    {
        debug!("[tag] List tags for {path:?}");
        let lines = super::new(
            [
                "for-each-ref",
                "--sort=-creatordate",
                "refs/tags/",
                "--format=%(refname:short) %(objectname:short) %(subject)",
            ],
            path,
            "List tags",
            mute,
        )
        .lines()?;

        let mut tags = Vec::with_capacity(lines.len());
        for line in lines {
            let fields = line.split(' ').collect::<Vec<_>>();
            if fields.len() < 3 {
                continue;
            }
            tags.push(Tag {
                name: fields[0].to_string(),
                commit_id: fields[1].to_string(),
                commit_message: fields[2..].join(" "),
            });
        }

        debug!("[tag] List result: {tags:?}");
        Ok(tags)
    }

    pub fn get<P>(path: Option<P>, mute: bool, name: &str) -> Result<Self>
    where
        P: AsRef<Path> + std::fmt::Debug,
    {
        debug!("[tag] Get tag {name} for {path:?}");
        let tags = Self::list(path, mute)?;
        for tag in tags {
            if tag.name == name {
                debug!("[tag] Found tag: {tag:?}");
                return Ok(tag);
            }
        }
        bail!("tag {name:?} not found");
    }

    pub fn get_latest<P>(path: Option<P>, mute: bool) -> Result<Self>
    where
        P: AsRef<Path> + std::fmt::Debug,
    {
        debug!("[tag] Get latest tag for {path:?}");
        let name = super::new(
            ["describe", "--tags", "--abbrev=0"],
            path.as_ref(),
            "Get latest tag",
            mute,
        )
        .output()?;
        if name.is_empty() {
            bail!("no latest tag");
        }
        Self::get(path, mute, &name)
    }
}

impl ListItem for Tag {
    fn row<'a>(&'a self, title: &str) -> Cow<'a, str> {
        let message = super::short_message(&self.commit_message);
        match title {
            "Name" => Cow::Borrowed(&self.name),
            "CommitID" => Cow::Borrowed(&self.commit_id),
            "Message" => message,
            _ => Cow::Borrowed(""),
        }
    }
}

impl List<Tag> for TagList {
    fn titles(&self) -> Vec<&'static str> {
        vec!["Name", "CommitID", "Message"]
    }

    fn total(&self) -> u32 {
        self.total
    }

    fn items(&self) -> &[Tag] {
        &self.tags
    }
}

#[cfg(test)]
mod tests {
    use crate::exec::git;

    use super::*;

    #[test]
    fn test_tag() {
        let Some(repo_path) = git::tests::setup() else {
            return;
        };

        let tags = Tag::list(Some(repo_path), true).unwrap();
        let mut targets = Vec::new();
        for tag in tags {
            match tag.name.as_str() {
                "v0.1.0" | "v0.2.0" | "v0.3.0" | "v0.4.0" | "v0.5.0" => {
                    targets.push(tag.name);
                }
                _ => {}
            }
        }
        assert_eq!(
            targets,
            vec![
                "v0.5.0".to_string(),
                "v0.4.0".to_string(),
                "v0.3.0".to_string(),
                "v0.2.0".to_string(),
                "v0.1.0".to_string()
            ]
        );
    }

    #[test]
    fn test_get() {
        let Some(repo_path) = git::tests::setup() else {
            return;
        };

        let tag = Tag::get(Some(repo_path), true, "v0.3.0").unwrap();
        assert_eq!(tag.name, "v0.3.0");
    }

    #[test]
    fn test_latest() {
        let Some(repo_path) = git::tests::setup() else {
            return;
        };

        let tags = Tag::list(Some(repo_path), true).unwrap();
        let latest = Tag::get_latest(Some(repo_path), true).unwrap();
        assert_eq!(latest, tags[0]);
    }
}

use std::borrow::Cow;

use anyhow::{Context, Result, bail};
use chrono::{Local, NaiveDate};
use semver::{Prerelease, Version};
use serde::Serialize;

use crate::debug;
use crate::term::list::{List, ListItem};

use super::GitCmd;

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
    pub fn list(cmd: GitCmd) -> Result<Vec<Self>> {
        debug!("[tag] List tags, cmd: {cmd:?}");
        let lines = cmd.lines(
            [
                "for-each-ref",
                "--sort=-creatordate",
                "refs/tags/",
                "--format=%(refname:short) %(objectname:short) %(subject)",
            ],
            "Listing tags",
        )?;

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

    pub fn get(cmd: GitCmd, name: &str) -> Result<Self> {
        debug!("[tag] Get tag {name}, cmd: {cmd:?}");
        let tags = Self::list(cmd)?;
        for tag in tags {
            if tag.name == name {
                debug!("[tag] Found tag: {tag:?}");
                return Ok(tag);
            }
        }
        bail!("tag {name:?} not found");
    }

    pub fn get_latest(cmd: GitCmd) -> Result<Self> {
        debug!("[tag] Get latest tag, cmd: {cmd:?}");
        let name = cmd.output(["describe", "--tags", "--abbrev=0"], "Getting latest tag")?;
        if name.is_empty() {
            bail!("no latest tag");
        }
        Self::get(cmd, &name)
    }
}

impl ListItem for Tag {
    fn row<'a>(&'a self, title: &str) -> Cow<'a, str> {
        match title {
            "Name" => Cow::Borrowed(&self.name),
            "CommitID" => Cow::Borrowed(&self.commit_id),
            "Message" => Cow::Borrowed(&self.commit_message),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateTagRule {
    Patch,
    Minor,
    Major,
    Date,
    DateDash,
    DateDot,
}

impl UpdateTagRule {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "patch" => Some(Self::Patch),
            "minor" => Some(Self::Minor),
            "major" => Some(Self::Major),
            "date" => Some(Self::Date),
            "date-dash" => Some(Self::DateDash),
            "date-dot" => Some(Self::DateDot),
            _ => None,
        }
    }

    pub fn apply(&self, mut tag: &str) -> Result<String> {
        match self {
            Self::Patch | Self::Minor | Self::Major => {
                let mut has_v_prefix = false;
                if tag.starts_with('v') {
                    has_v_prefix = true;
                    tag = &tag[1..];
                }

                let mut version = Version::parse(tag)
                    .with_context(|| format!("failed to parse tag {tag:?} as semver"))?;
                match self {
                    Self::Patch => version.patch += 1,
                    Self::Minor => {
                        version.minor += 1;
                        version.patch = 0;
                    }
                    Self::Major => {
                        version.major += 1;
                        version.minor = 0;
                        version.patch = 0;
                    }
                    _ => unreachable!(),
                }
                version.pre = Prerelease::EMPTY;
                let version = if has_v_prefix {
                    format!("v{version}")
                } else {
                    version.to_string()
                };
                Ok(version)
            }
            Self::Date => {
                let format = if NaiveDate::parse_from_str(tag, "%Y-%m-%d").is_ok() {
                    "%Y-%m-%d"
                } else if NaiveDate::parse_from_str(tag, "%Y.%m.%d").is_ok() {
                    "%Y.%m.%d"
                } else {
                    bail!(
                        "tag {tag:?} is not a valid date format (YYYY-MM-DD or YYYY.MM.DD), please use `date-dash` or `date-dot` rule instead"
                    );
                };

                let date = Local::now().format(format).to_string();
                Ok(date)
            }
            Self::DateDash => {
                let date = Local::now().format("%Y-%m-%d").to_string();
                Ok(date)
            }
            Self::DateDot => {
                let date = Local::now().format("%Y.%m.%d").to_string();
                Ok(date)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::config::context::ConfigContext;
    use crate::exec::git;

    use super::*;

    #[test]
    fn test_tag() {
        let Some(repo_path) = git::tests::setup() else {
            return;
        };
        let ctx = ConfigContext::new_mock();
        let cmd = ctx.git_work_dir(&repo_path);

        let tags = Tag::list(cmd).unwrap();
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
        let ctx = ConfigContext::new_mock();
        let cmd = ctx.git_work_dir(&repo_path);

        let tag = Tag::get(cmd, "v0.3.0").unwrap();
        assert_eq!(tag.name, "v0.3.0");
    }

    #[test]
    fn test_latest() {
        let Some(repo_path) = git::tests::setup() else {
            return;
        };
        let ctx = ConfigContext::new_mock();
        let cmd = ctx.git_work_dir(&repo_path);

        let tags = Tag::list(cmd).unwrap();
        let latest = Tag::get_latest(cmd).unwrap();
        assert_eq!(latest, tags[0]);
    }

    #[test]
    fn test_update_tag() {
        struct Case {
            input: &'static str,
            expect: Option<UpdateTagRule>,
        }

        let from_str_cases = vec![
            Case {
                input: "patch",
                expect: Some(UpdateTagRule::Patch),
            },
            Case {
                input: "minor",
                expect: Some(UpdateTagRule::Minor),
            },
            Case {
                input: "major",
                expect: Some(UpdateTagRule::Major),
            },
            Case {
                input: "date-dash",
                expect: Some(UpdateTagRule::DateDash),
            },
            Case {
                input: "date-dot",
                expect: Some(UpdateTagRule::DateDot),
            },
            Case {
                input: "invalid",
                expect: None,
            },
            Case {
                input: "",
                expect: None,
            },
            Case {
                input: "PATCH",
                expect: None,
            },
        ];

        for case in from_str_cases {
            assert_eq!(UpdateTagRule::from_str(case.input), case.expect);
        }

        #[derive(Debug)]
        struct ApplyCase {
            rule: UpdateTagRule,
            input: &'static str,
            expect: &'static str,
        }

        let apply_cases = vec![
            ApplyCase {
                rule: UpdateTagRule::Patch,
                input: "1.0.0",
                expect: "1.0.1",
            },
            ApplyCase {
                rule: UpdateTagRule::Patch,
                input: "v1.0.0",
                expect: "v1.0.1",
            },
            ApplyCase {
                rule: UpdateTagRule::Patch,
                input: "0.1.5",
                expect: "0.1.6",
            },
            ApplyCase {
                rule: UpdateTagRule::Patch,
                input: "v2.3.9",
                expect: "v2.3.10",
            },
            ApplyCase {
                rule: UpdateTagRule::Patch,
                input: "0.0.0",
                expect: "0.0.1",
            },
            ApplyCase {
                rule: UpdateTagRule::Patch,
                input: "v0.0.0",
                expect: "v0.0.1",
            },
            ApplyCase {
                rule: UpdateTagRule::Minor,
                input: "1.0.0",
                expect: "1.1.0",
            },
            ApplyCase {
                rule: UpdateTagRule::Minor,
                input: "v1.0.0",
                expect: "v1.1.0",
            },
            ApplyCase {
                rule: UpdateTagRule::Minor,
                input: "0.1.5",
                expect: "0.2.0",
            },
            ApplyCase {
                rule: UpdateTagRule::Minor,
                input: "v2.3.9",
                expect: "v2.4.0",
            },
            ApplyCase {
                rule: UpdateTagRule::Minor,
                input: "0.0.0",
                expect: "0.1.0",
            },
            ApplyCase {
                rule: UpdateTagRule::Minor,
                input: "v0.0.0",
                expect: "v0.1.0",
            },
            ApplyCase {
                rule: UpdateTagRule::Major,
                input: "1.0.0",
                expect: "2.0.0",
            },
            ApplyCase {
                rule: UpdateTagRule::Minor,
                input: "1.0.0-alpha.5",
                expect: "1.1.0",
            },
            ApplyCase {
                rule: UpdateTagRule::Minor,
                input: "v1.0.0-beta.1",
                expect: "v1.1.0",
            },
            ApplyCase {
                rule: UpdateTagRule::Minor,
                input: "2.1.0-rc.3",
                expect: "2.2.0",
            },
            ApplyCase {
                rule: UpdateTagRule::Major,
                input: "1.0.0-alpha.5",
                expect: "2.0.0",
            },
            ApplyCase {
                rule: UpdateTagRule::Major,
                input: "v1.0.0-beta.1",
                expect: "v2.0.0",
            },
            ApplyCase {
                rule: UpdateTagRule::Major,
                input: "2.1.0-rc.3",
                expect: "3.0.0",
            },
            ApplyCase {
                rule: UpdateTagRule::Patch,
                input: "invalid",
                expect: "",
            },
            ApplyCase {
                rule: UpdateTagRule::Patch,
                input: "1.0",
                expect: "",
            },
            ApplyCase {
                rule: UpdateTagRule::Patch,
                input: "1.0.0.0",
                expect: "",
            },
            ApplyCase {
                rule: UpdateTagRule::Patch,
                input: "",
                expect: "",
            },
        ];

        for case in apply_cases {
            let result = case.rule.apply(case.input);
            if case.expect.is_empty() {
                assert!(result.is_err(), "case: {case:?}");
            } else {
                assert_eq!(result.unwrap(), case.expect, "case: {case:?}");
            }
        }

        let date_dash_result = UpdateTagRule::DateDash.apply("any_tag").unwrap();
        let today_dash = chrono::Local::now().format("%Y-%m-%d").to_string();
        assert_eq!(date_dash_result, today_dash);

        let date_dot_result = UpdateTagRule::DateDot.apply("any_tag").unwrap();
        let today_dot = chrono::Local::now().format("%Y.%m.%d").to_string();
        assert_eq!(date_dot_result, today_dot);

        let date_result = UpdateTagRule::Date.apply("2020-12-12").unwrap();
        assert_eq!(date_result, today_dash);

        let date_result = UpdateTagRule::Date.apply("2020.12.12").unwrap();
        assert_eq!(date_result, today_dot);
    }
}

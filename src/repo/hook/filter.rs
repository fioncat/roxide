use std::path::Path;

use anyhow::{Result, bail};

use crate::db::repo::Repository;
use crate::scan::ignore::Ignore;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filter {
    pub remote: Option<String>,
    pub owner: Option<String>,
    pub name: Option<String>,

    pub exclude: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterResult {
    Ok,
    NotMatched,
    Exclude,
}

impl Filter {
    pub fn parse(mut s: &str) -> Result<Self> {
        s = s.trim();
        let mut exclude = false;
        if s.starts_with('!') {
            exclude = true;
            s = &s[1..];
            s = s.trim();
        }

        let fields = s.split_whitespace().collect::<Vec<_>>();
        match fields.len() {
            0 => bail!("filter pattern cannot be empty"),
            1 => {
                let remote = fields[0];
                Ok(Self {
                    remote: Some(remote.to_string()),
                    owner: None,
                    name: None,
                    exclude,
                })
            }
            2 => {
                let remote = fields[0];
                let owner = fields[1];
                Ok(Self {
                    remote: Some(remote.to_string()),
                    owner: Some(owner.to_string()),
                    name: None,
                    exclude,
                })
            }
            3 => {
                let remote = fields[0];
                let owner = fields[1];
                let name = fields[2];
                Ok(Self {
                    remote: Some(remote.to_string()),
                    owner: Some(owner.to_string()),
                    name: Some(name.to_string()),
                    exclude,
                })
            }
            _ => bail!("filter pattern has too many fields"),
        }
    }

    pub fn matched(&self, repo: &Repository) -> FilterResult {
        let remote_result = self.matched_field(&self.remote, &repo.remote);
        if !matches!(remote_result, FilterResult::Ok) {
            return remote_result;
        }

        let owner_result = self.matched_field(&self.owner, &repo.owner);
        if !matches!(owner_result, FilterResult::Ok) {
            return owner_result;
        }

        self.matched_field(&self.name, &repo.name)
    }

    fn matched_field(&self, pattern: &Option<String>, field: &str) -> FilterResult {
        let matched = if let Some(pattern) = pattern.as_deref() {
            if pattern == "*" {
                true
            } else if pattern.contains('*') {
                match Ignore::parse(Path::new(""), [pattern]) {
                    Ok(ignore) => ignore.matched(Path::new(field), false),
                    Err(_) => false,
                }
            } else {
                pattern == field
            }
        } else {
            true
        };

        if matched {
            if self.exclude {
                FilterResult::Exclude
            } else {
                FilterResult::Ok
            }
        } else if self.exclude {
            FilterResult::Ok
        } else {
            FilterResult::NotMatched
        }
    }
}

pub fn matched_filters(repo: &Repository, filters: &[Filter]) -> bool {
    let mut matched = false;
    for filter in filters {
        match filter.matched(repo) {
            FilterResult::Ok => matched = true,
            FilterResult::Exclude => return false,
            FilterResult::NotMatched => continue,
        }
    }
    matched
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse() {
        struct Case {
            pattern: &'static str,
            expect: Option<Filter>,
        }

        let cases = [
            Case {
                pattern: "",
                expect: None,
            },
            Case {
                pattern: "*",
                expect: Some(Filter {
                    remote: Some("*".to_string()),
                    ..Default::default()
                }),
            },
            Case {
                pattern: "! github",
                expect: Some(Filter {
                    remote: Some("github".to_string()),
                    exclude: true,
                    ..Default::default()
                }),
            },
            Case {
                pattern: "github fioncat",
                expect: Some(Filter {
                    remote: Some("github".to_string()),
                    owner: Some("fioncat".to_string()),
                    ..Default::default()
                }),
            },
            Case {
                pattern: "github fioncat roxide",
                expect: Some(Filter {
                    remote: Some("github".to_string()),
                    owner: Some("fioncat".to_string()),
                    name: Some("roxide".to_string()),
                    ..Default::default()
                }),
            },
            Case {
                pattern: "github fioncat roxide extra",
                expect: None,
            },
            Case {
                pattern: "! github fioncat",
                expect: Some(Filter {
                    remote: Some("github".to_string()),
                    owner: Some("fioncat".to_string()),
                    exclude: true,
                    ..Default::default()
                }),
            },
            Case {
                pattern: "github kubernetes kube-*",
                expect: Some(Filter {
                    remote: Some("github".to_string()),
                    owner: Some("kubernetes".to_string()),
                    name: Some("kube-*".to_string()),
                    exclude: false,
                }),
            },
            Case {
                pattern: "! github * kube*",
                expect: Some(Filter {
                    remote: Some("github".to_string()),
                    owner: Some("*".to_string()),
                    name: Some("kube*".to_string()),
                    exclude: true,
                }),
            },
        ];

        for case in cases {
            let result = Filter::parse(case.pattern);
            match case.expect {
                Some(expect) => {
                    let filter = result.expect("parse filter should succeed");
                    assert_eq!(filter, expect, "pattern: {}", case.pattern);
                }
                None => {
                    assert!(result.is_err(), "pattern: {}", case.pattern);
                }
            }
        }
    }

    #[test]
    fn test_matched() {
        let repo = Repository {
            remote: "github".to_string(),
            owner: "fioncat".to_string(),
            name: "roxide".to_string(),
            ..Default::default()
        };

        struct Case {
            patterns: Vec<&'static str>,
            expect: bool,
        }

        let cases = [
            Case {
                patterns: vec!["github"],
                expect: true,
            },
            Case {
                patterns: vec!["github fioncat"],
                expect: true,
            },
            Case {
                patterns: vec!["github fioncat roxide"],
                expect: true,
            },
            Case {
                patterns: vec!["github", "! github fioncat"],
                expect: false,
            },
            Case {
                patterns: vec!["github", "! github fioncat roxide"],
                expect: false,
            },
            Case {
                patterns: vec!["github fioncat rox*"],
                expect: true,
            },
            Case {
                patterns: vec!["github * roxide"],
                expect: true,
            },
            Case {
                patterns: vec!["github kubernetes", "test golang", "test rust"],
                expect: false,
            },
            Case {
                patterns: vec!["github kubernetes", "github fioncat"],
                expect: true,
            },
        ];

        for case in cases {
            let filters = case
                .patterns
                .iter()
                .map(|p| Filter::parse(p).expect("valid filter"))
                .collect::<Vec<_>>();
            let result = matched_filters(&repo, &filters);
            assert_eq!(result, case.expect, "patterns: {:?}", case.patterns);
        }
    }
}

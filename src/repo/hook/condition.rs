use anyhow::{Result, bail};

use crate::exec::git::commit::count_uncommitted_changes;
use crate::format::now;
use crate::repo::ops::{CreateResult, RepoOperator};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Condition {
    Created,
    Cloned,
    Updated,
    Interval(u64),
}

impl Condition {
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "created" => Ok(Self::Created),
            "cloned" => Ok(Self::Cloned),
            "updated" => Ok(Self::Updated),
            _ => {
                let d = parse_duration(s)?;
                Ok(Self::Interval(d))
            }
        }
    }

    pub fn matched(
        &self,
        op: &RepoOperator,
        name: &str,
        create_result: CreateResult,
    ) -> Result<bool> {
        match self {
            Self::Created => Ok(matches!(create_result, CreateResult::Created)),
            Self::Cloned => Ok(matches!(create_result, CreateResult::Cloned)),
            Self::Updated => {
                let count = count_uncommitted_changes(op.git().mute())?;
                Ok(count > 0)
            }
            Self::Interval(interval) => {
                let db = op.ctx().get_db()?;
                db.with_transaction(|tx| {
                    match tx.hook_history().get(op.repo().id, name)? {
                        Some(history) => {
                            let now = now();
                            Ok(now >= history.time + interval)
                        }
                        None => {
                            // never executed, always match
                            Ok(true)
                        }
                    }
                })
            }
        }
    }
}

pub fn parse_duration(s: &str) -> Result<u64> {
    if s.is_empty() {
        bail!("duration string cannot be empty");
    }

    let s = s.trim();
    if s.len() < 2 {
        bail!("invalid duration format: {s:?}");
    }

    let (number_part, unit_part) = s.split_at(s.len() - 1);

    let number: u64 = number_part
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid number in duration: {}", number_part))?;

    let seconds = match unit_part {
        "s" => number,
        "m" => number * 60,
        "h" => number * 3600,
        "d" => number * 86400,
        _ => bail!("unsupported time unit: {unit_part:?}. Supported units: s, m, h, d"),
    };

    Ok(seconds)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::config::context;
    use crate::db::hook_history::HookHistory;
    use crate::db::repo::Repository;

    use super::*;

    #[test]
    fn test_parse() {
        #[derive(Debug)]
        struct Case {
            pattern: &'static str,
            expect: Condition,
        }

        let cases = [
            Case {
                pattern: "created",
                expect: Condition::Created,
            },
            Case {
                pattern: "cloned",
                expect: Condition::Cloned,
            },
            Case {
                pattern: "updated",
                expect: Condition::Updated,
            },
            Case {
                pattern: "20s",
                expect: Condition::Interval(20),
            },
            Case {
                pattern: "3m",
                expect: Condition::Interval(3 * 60),
            },
            Case {
                pattern: "10m",
                expect: Condition::Interval(10 * 60),
            },
            Case {
                pattern: "10h",
                expect: Condition::Interval(10 * 3600),
            },
            Case {
                pattern: "24h",
                expect: Condition::Interval(24 * 3600),
            },
            Case {
                pattern: "1d",
                expect: Condition::Interval(24 * 3600),
            },
            Case {
                pattern: "10d",
                expect: Condition::Interval(10 * 24 * 3600),
            },
            Case {
                pattern: "365d",
                expect: Condition::Interval(365 * 24 * 3600),
            },
        ];

        for case in cases {
            let cond = Condition::parse(case.pattern).unwrap();
            assert_eq!(case.expect, cond, "{case:?}");
        }
    }

    #[test]
    fn test_matched_create() {
        let ctx = context::tests::build_test_context("hook_condition_matched_create");
        let repo = Repository {
            remote: "github".to_string(),
            owner: "fioncat".to_string(),
            name: "roxide".to_string(),
            ..Default::default()
        };
        let op = RepoOperator::load(&ctx, &repo).unwrap();

        let cond = Condition::Created;
        assert!(cond.matched(&op, "", CreateResult::Created).unwrap());
        assert!(!cond.matched(&op, "", CreateResult::Cloned).unwrap());
        assert!(!cond.matched(&op, "", CreateResult::Exists).unwrap());

        let cond = Condition::Cloned;
        assert!(cond.matched(&op, "", CreateResult::Cloned).unwrap());
        assert!(!cond.matched(&op, "", CreateResult::Created).unwrap());
        assert!(!cond.matched(&op, "", CreateResult::Exists).unwrap());
    }

    #[test]
    fn test_matched_update() {
        let ctx = context::tests::build_test_context("hook_condition_matched_update");
        let repo = Repository {
            remote: "test".to_string(),
            owner: "rust".to_string(),
            name: "hello".to_string(),
            ..Default::default()
        };

        let op = RepoOperator::load(&ctx, &repo).unwrap();
        op.ensure_create(false, None).unwrap();

        op.git()
            .execute(["config", "user.name", "test"], "")
            .unwrap();
        op.git()
            .execute(["config", "user.email", "test@sample.com"], "")
            .unwrap();

        let path = op.path().join("origin.txt");
        fs::write(path, "origin content").unwrap();

        op.git().execute(["add", "."], "").unwrap();
        op.git().execute(["commit", "-m", "test"], "").unwrap();

        let cond = Condition::Updated;
        assert!(!cond.matched(&op, "", CreateResult::Exists).unwrap());

        let path = op.path().join("test.txt");
        fs::write(path, "test content").unwrap();

        assert!(cond.matched(&op, "", CreateResult::Exists).unwrap());
    }

    #[test]
    fn test_matched_interval() {
        let ctx = context::tests::build_test_context("hook_condition_matched_interval");
        let repo = Repository {
            id: 1,
            remote: "github".to_string(),
            owner: "fioncat".to_string(),
            name: "roxide".to_string(),
            ..Default::default()
        };
        let op = RepoOperator::load(&ctx, &repo).unwrap();

        let cond = Condition::Interval(100);

        assert!(cond.matched(&op, "test", CreateResult::Exists).unwrap());
        assert!(
            cond.matched(&op, "spell-check", CreateResult::Exists)
                .unwrap()
        );
        let db = ctx.get_db().unwrap();
        db.with_transaction(|tx| {
            tx.hook_history().update(&HookHistory {
                repo_id: op.repo().id,
                name: "spell-check".to_string(),
                success: true,
                time: now(),
            })
        })
        .unwrap();

        assert!(
            !cond
                .matched(&op, "spell-check", CreateResult::Exists)
                .unwrap()
        );
    }
}

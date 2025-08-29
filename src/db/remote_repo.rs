use std::borrow::Cow;

use anyhow::Result;
use rusqlite::{OptionalExtension, Row, Transaction, params};

use crate::debug;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RemoteRepository<'a> {
    pub remote: Cow<'a, str>,
    pub owner: Cow<'a, str>,
    pub name: Cow<'a, str>,

    pub default_branch: String,

    pub web_url: String,

    pub upstream: Option<RemoteUpstream>,

    pub expire_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteUpstream {
    pub owner: String,
    pub name: String,
    pub default_branch: String,
}

impl<'a> RemoteRepository<'a> {
    fn from_row(row: &Row) -> rusqlite::Result<Self> {
        let upstream_owner: Option<String> = row.get(5)?;
        let upstream_name: Option<String> = row.get(6)?;
        let upstream_default_branch: Option<String> = row.get(7)?;
        let upstream = match (upstream_owner, upstream_name, upstream_default_branch) {
            (Some(owner), Some(name), Some(default_branch)) => Some(RemoteUpstream {
                owner,
                name,
                default_branch,
            }),
            _ => None,
        };
        Ok(Self {
            remote: row.get(0)?,
            owner: row.get(1)?,
            name: row.get(2)?,
            default_branch: row.get(3)?,
            web_url: row.get(4)?,
            upstream,
            expire_at: row.get(8)?,
        })
    }
}

pub struct RemoteRepositoryHandle<'a> {
    tx: &'a Transaction<'a>,
}

impl<'a> RemoteRepositoryHandle<'a> {
    pub fn new(tx: &'a Transaction) -> Self {
        Self { tx }
    }

    pub fn ensure_table(&self) -> Result<()> {
        ensure_table(self.tx)
    }

    pub fn insert(&self, repo: &RemoteRepository) -> Result<()> {
        insert(self.tx, repo)
    }

    pub fn get_optional(
        &self,
        remote: &str,
        owner: &str,
        name: &str,
    ) -> Result<Option<RemoteRepository<'static>>> {
        get_optional(self.tx, remote, owner, name)
    }

    pub fn delete(&self, repo: &RemoteRepository) -> Result<()> {
        delete(self.tx, repo)
    }
}

const TABLE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS remote_repo (
    remote TEXT NOT NULL,
    owner TEXT NOT NULL,
    name TEXT NOT NULL,
    default_branch TEXT NOT NULL,
    web_url TEXT NOT NULL,
    upstream_owner TEXT,
    upstream_name TEXT,
    upstream_default_branch TEXT,
    expire_at INTEGER NOT NULL,
    PRIMARY KEY (remote, owner, name)
);
"#;

fn ensure_table(tx: &Transaction) -> Result<()> {
    debug!("[db] Ensure remote_repo table exists");
    tx.execute(TABLE_SQL, params![])?;
    Ok(())
}

const INSERT_SQL: &str = r#"
INSERT INTO remote_repo (
    remote,
    owner,
    name,
    default_branch,
    web_url,
    upstream_owner,
    upstream_name,
    upstream_default_branch,
    expire_at
) VALUES (
    ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9
)
"#;

fn insert(tx: &Transaction, repo: &RemoteRepository) -> Result<()> {
    debug!("[db] Insert remote_repo: {repo:?}");
    tx.execute(
        INSERT_SQL,
        params![
            repo.remote,
            repo.owner,
            repo.name,
            repo.default_branch,
            repo.web_url,
            repo.upstream.as_ref().map(|u| &u.owner),
            repo.upstream.as_ref().map(|u| &u.name),
            repo.upstream.as_ref().map(|u| &u.default_branch),
            repo.expire_at
        ],
    )?;
    Ok(())
}

const GET_SQL: &str = r#"
SELECT remote, owner, name, default_branch, web_url, upstream_owner, upstream_name, upstream_default_branch, expire_at
FROM remote_repo
WHERE remote = ?1 AND owner = ?2 AND name = ?3
"#;

fn get_optional(
    tx: &Transaction,
    remote: &str,
    owner: &str,
    name: &str,
) -> Result<Option<RemoteRepository<'static>>> {
    debug!("[db] Get remote_repo: {remote}:{owner}:{name}");
    let mut stmt = tx.prepare(GET_SQL)?;
    let repo = stmt
        .query_row(params![remote, owner, name], RemoteRepository::from_row)
        .optional()?;
    debug!("[db] Result: {repo:?}");
    Ok(repo)
}

const DELETE_SQL: &str = r#"
DELETE FROM remote_repo
WHERE remote = ?1 AND owner = ?2 AND name = ?3
"#;

fn delete(tx: &Transaction, repo: &RemoteRepository) -> Result<()> {
    debug!("[db] Delete remote_repo: {repo:?}");
    tx.execute(DELETE_SQL, params![repo.remote, repo.owner, repo.name])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use super::*;

    fn build_conn() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        let tx = conn.transaction().unwrap();
        ensure_table(&tx).unwrap();
        let repos = test_remote_repos();
        for repo in repos {
            insert(&tx, &repo).unwrap();
        }
        tx.commit().unwrap();
        conn
    }

    fn test_remote_repos() -> Vec<RemoteRepository<'static>> {
        vec![
            RemoteRepository {
                remote: Cow::Owned("github".to_string()),
                owner: Cow::Owned("fioncat".to_string()),
                name: Cow::Owned("roxide".to_string()),
                default_branch: "main".to_string(),
                web_url: "https://github.com/fioncat/roxide".to_string(),
                upstream: None,
                expire_at: 1234567890,
            },
            RemoteRepository {
                remote: Cow::Owned("github".to_string()),
                owner: Cow::Owned("alice".to_string()),
                name: Cow::Owned("myproject".to_string()),
                default_branch: "master".to_string(),
                web_url: "https://github.com/alice/myproject".to_string(),
                upstream: Some(RemoteUpstream {
                    owner: "upstream".to_string(),
                    name: "myproject".to_string(),
                    default_branch: "main".to_string(),
                }),
                expire_at: 1234567800,
            },
            RemoteRepository {
                remote: Cow::Owned("gitlab".to_string()),
                owner: Cow::Owned("bob".to_string()),
                name: Cow::Owned("project1".to_string()),
                default_branch: "develop".to_string(),
                web_url: "https://gitlab.com/bob/project1".to_string(),
                upstream: None,
                expire_at: 1234567700,
            },
            RemoteRepository {
                remote: Cow::Owned("github".to_string()),
                owner: Cow::Owned("contributor".to_string()),
                name: Cow::Owned("kubernetes".to_string()),
                default_branch: "master".to_string(),
                web_url: "https://github.com/contributor/kubernetes".to_string(),
                upstream: Some(RemoteUpstream {
                    owner: "kubernetes".to_string(),
                    name: "kubernetes".to_string(),
                    default_branch: "master".to_string(),
                }),
                expire_at: 1234567600,
            },
        ]
    }

    #[test]
    fn test_insert() {
        let mut conn = build_conn();
        let tx = conn.transaction().unwrap();
        let repos = test_remote_repos();
        for repo in repos {
            let result = get_optional(&tx, &repo.remote, &repo.owner, &repo.name).unwrap();
            assert_eq!(result, Some(repo));
        }
    }

    #[test]
    fn test_get_not_found() {
        let mut conn = build_conn();
        let tx = conn.transaction().unwrap();
        let result = get_optional(&tx, "not_exist", "not_exist", "not_exist").unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_delete() {
        let mut conn = build_conn();
        let repos = test_remote_repos();
        for repo in repos {
            let tx = conn.transaction().unwrap();
            delete(&tx, &repo).unwrap();
            tx.commit().unwrap();
            let tx = conn.transaction().unwrap();
            let result = get_optional(&tx, &repo.remote, &repo.owner, &repo.name).unwrap();
            assert_eq!(result, None);
        }
    }
}

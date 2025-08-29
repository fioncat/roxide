use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::debug;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteRepository {
    pub remote: String,
    pub owner: String,
    pub name: String,

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

impl RemoteRepository {
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

#[allow(dead_code)] // TODO: remove this
pub struct RemoteRepositoryHandle<'a> {
    conn: &'a Connection,
}

#[allow(dead_code)] // TODO: remove this
impl<'a> RemoteRepositoryHandle<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn ensure_table(&self) -> Result<()> {
        ensure_table(self.conn)
    }

    pub fn insert(&self, repo: &RemoteRepository) -> Result<()> {
        insert(self.conn, repo)
    }

    pub fn get_optional(
        &self,
        remote: &str,
        owner: &str,
        name: &str,
    ) -> Result<Option<RemoteRepository>> {
        get_optional(self.conn, remote, owner, name)
    }

    pub fn delete(&self, repo: &RemoteRepository) -> Result<()> {
        delete(self.conn, repo)
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

fn ensure_table(conn: &Connection) -> Result<()> {
    debug!("[db] Ensure remote_repo table exists");
    conn.execute(TABLE_SQL, params![])?;
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

fn insert(conn: &Connection, repo: &RemoteRepository) -> Result<()> {
    debug!("[db] Insert remote_repo: {repo:?}");
    conn.execute(
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
    conn: &Connection,
    remote: &str,
    owner: &str,
    name: &str,
) -> Result<Option<RemoteRepository>> {
    debug!("[db] Get remote_repo: {remote}:{owner}:{name}");
    let mut stmt = conn.prepare(GET_SQL)?;
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

fn delete(conn: &Connection, repo: &RemoteRepository) -> Result<()> {
    debug!("[db] Delete remote_repo: {repo:?}");
    conn.execute(DELETE_SQL, params![repo.remote, repo.owner, repo.name])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        ensure_table(&conn).unwrap();
        let repos = test_remote_repos();
        for repo in repos {
            insert(&conn, &repo).unwrap();
        }
        conn
    }

    fn test_remote_repos() -> Vec<RemoteRepository> {
        vec![
            RemoteRepository {
                remote: "github".to_string(),
                owner: "fioncat".to_string(),
                name: "roxide".to_string(),
                default_branch: "main".to_string(),
                web_url: "https://github.com/fioncat/roxide".to_string(),
                upstream: None,
                expire_at: 1234567890,
            },
            RemoteRepository {
                remote: "github".to_string(),
                owner: "alice".to_string(),
                name: "myproject".to_string(),
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
                remote: "gitlab".to_string(),
                owner: "bob".to_string(),
                name: "project1".to_string(),
                default_branch: "develop".to_string(),
                web_url: "https://gitlab.com/bob/project1".to_string(),
                upstream: None,
                expire_at: 1234567700,
            },
            RemoteRepository {
                remote: "github".to_string(),
                owner: "contributor".to_string(),
                name: "kubernetes".to_string(),
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
        let conn = build_conn();
        let repos = test_remote_repos();
        for repo in repos {
            let result = get_optional(&conn, &repo.remote, &repo.owner, &repo.name).unwrap();
            assert_eq!(result, Some(repo));
        }
    }

    #[test]
    fn test_get_not_found() {
        let conn = build_conn();
        let result = get_optional(&conn, "not_exist", "not_exist", "not_exist").unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_delete() {
        let conn = build_conn();
        let repos = test_remote_repos();
        for repo in repos {
            delete(&conn, &repo).unwrap();
            let result = get_optional(&conn, &repo.remote, &repo.owner, &repo.name).unwrap();
            assert_eq!(result, None);
        }
    }
}

use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::debug;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteOwner {
    pub remote: String,
    pub owner: String,

    pub repos: Vec<String>,

    pub expire_at: u64,
}

impl RemoteOwner {
    fn from_row(row: &Row) -> rusqlite::Result<Self> {
        let repos = row
            .get::<_, String>(2)?
            .split(',')
            .map(|s| s.to_string())
            .collect::<Vec<String>>();
        Ok(Self {
            remote: row.get(0)?,
            owner: row.get(1)?,
            repos,
            expire_at: row.get(3)?,
        })
    }
}

#[allow(dead_code)] // TODO: remove this
pub struct RemoteOwnerHandle<'a> {
    conn: &'a Connection,
}

#[allow(dead_code)] // TODO: remove this
impl<'a> RemoteOwnerHandle<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn ensure_table(&self) -> Result<()> {
        ensure_table(self.conn)
    }

    pub fn insert(&self, owner: &RemoteOwner) -> Result<()> {
        insert(self.conn, owner)
    }

    pub fn get_optional(&self, remote: &str, owner: &str) -> Result<Option<RemoteOwner>> {
        get_optional(self.conn, remote, owner)
    }

    pub fn delete(&self, owner: &RemoteOwner) -> Result<()> {
        delete(self.conn, owner)
    }
}

const TABLE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS remote_owner (
    remote TEXT NOT NULL,
    owner TEXT NOT NULL,
    repos TEXT NOT NULL,
    expire_at INTEGER NOT NULL,
    PRIMARY KEY (remote, owner)
);
"#;

fn ensure_table(conn: &rusqlite::Connection) -> Result<()> {
    debug!("[db] Ensure remote_owner table exists");
    conn.execute_batch(TABLE_SQL)?;
    Ok(())
}

const INSERT_SQL: &str = r#"
INSERT INTO remote_owner (remote, owner, repos, expire_at)
VALUES (?1, ?2, ?3, ?4)
"#;

fn insert(conn: &Connection, owner: &RemoteOwner) -> Result<()> {
    debug!("[db] Insert remote_owner: {owner:?}");
    conn.execute(
        INSERT_SQL,
        params![
            owner.remote,
            owner.owner,
            owner.repos.join(","),
            owner.expire_at
        ],
    )?;
    Ok(())
}

const GET_SQL: &str = r#"
SELECT remote, owner, repos, expire_at
FROM remote_owner
WHERE remote = ?1 AND owner = ?2
"#;

fn get_optional(conn: &Connection, remote: &str, owner: &str) -> Result<Option<RemoteOwner>> {
    debug!("[db] Get remote_owner: {remote}:{owner}");
    let mut stmt = conn.prepare(GET_SQL)?;
    let remote_owner = stmt
        .query_row(params![remote, owner], RemoteOwner::from_row)
        .optional()?;
    debug!("[db] Result: {remote_owner:?}");
    Ok(remote_owner)
}

const DELETE_SQL: &str = r#"
DELETE FROM remote_owner
WHERE remote = ?1 AND owner = ?2
"#;

fn delete(conn: &Connection, remote_owner: &RemoteOwner) -> Result<()> {
    debug!("[db] Delete remote_owner: {remote_owner:?}");
    conn.execute(DELETE_SQL, params![remote_owner.remote, remote_owner.owner])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        ensure_table(&conn).unwrap();
        let owners = test_remote_owners();
        for owner in owners {
            insert(&conn, &owner).unwrap();
        }
        conn
    }

    fn test_remote_owners() -> Vec<RemoteOwner> {
        vec![
            RemoteOwner {
                remote: "github".to_string(),
                owner: "alice".to_string(),
                repos: vec!["repo1".to_string(), "repo2".to_string()],
                expire_at: 12345,
            },
            RemoteOwner {
                remote: "gitlab".to_string(),
                owner: "bob".to_string(),
                repos: vec!["project1".to_string()],
                expire_at: 6789,
            },
        ]
    }

    #[test]
    fn test_insert() {
        let conn = build_conn();
        let owners = test_remote_owners();
        for owner in owners {
            let result = get_optional(&conn, &owner.remote, &owner.owner).unwrap();
            assert_eq!(result, Some(owner));
        }
    }

    #[test]
    fn test_get_not_found() {
        let conn = build_conn();
        let owner = get_optional(&conn, "unknown", "nobody").unwrap();
        assert_eq!(owner, None);
    }

    #[test]
    fn test_delete() {
        let conn = build_conn();
        let owners = test_remote_owners();
        for owner in owners {
            delete(&conn, &owner).unwrap();
            let owner = get_optional(&conn, &owner.remote, &owner.owner).unwrap();
            assert_eq!(owner, None);
        }
    }
}

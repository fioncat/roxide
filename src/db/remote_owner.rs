use std::borrow::Cow;

use anyhow::Result;
use rusqlite::{OptionalExtension, Row, Transaction, params};

use crate::debug;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteOwner<'a> {
    pub remote: Cow<'a, str>,
    pub owner: Cow<'a, str>,

    pub repos: Vec<String>,

    pub expire_at: u64,
}

impl<'a> RemoteOwner<'a> {
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

pub struct RemoteOwnerHandle<'a> {
    tx: &'a Transaction<'a>,
}

impl<'a> RemoteOwnerHandle<'a> {
    pub fn new(tx: &'a Transaction<'a>) -> Self {
        Self { tx }
    }

    pub fn ensure_table(&self) -> Result<()> {
        ensure_table(self.tx)
    }

    pub fn insert(&self, owner: &RemoteOwner) -> Result<()> {
        insert(self.tx, owner)
    }

    pub fn get_optional(&self, remote: &str, owner: &str) -> Result<Option<RemoteOwner<'static>>> {
        get_optional(self.tx, remote, owner)
    }

    pub fn delete(&self, owner: &RemoteOwner) -> Result<()> {
        delete(self.tx, owner)
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

fn ensure_table(tx: &Transaction) -> Result<()> {
    debug!("[db] Ensure remote_owner table exists");
    tx.execute_batch(TABLE_SQL)?;
    Ok(())
}

const INSERT_SQL: &str = r#"
INSERT INTO remote_owner (remote, owner, repos, expire_at)
VALUES (?1, ?2, ?3, ?4)
"#;

fn insert(tx: &Transaction, owner: &RemoteOwner) -> Result<()> {
    debug!("[db] Insert remote_owner: {owner:?}");
    tx.execute(
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

fn get_optional(
    tx: &Transaction,
    remote: &str,
    owner: &str,
) -> Result<Option<RemoteOwner<'static>>> {
    debug!("[db] Get remote_owner: {remote}:{owner}");
    let mut stmt = tx.prepare(GET_SQL)?;
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

fn delete(tx: &Transaction, remote_owner: &RemoteOwner) -> Result<()> {
    debug!("[db] Delete remote_owner: {remote_owner:?}");
    tx.execute(DELETE_SQL, params![remote_owner.remote, remote_owner.owner])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use super::*;

    fn test_remote_owners() -> Vec<RemoteOwner<'static>> {
        vec![
            RemoteOwner {
                remote: Cow::Owned("github".to_string()),
                owner: Cow::Owned("alice".to_string()),
                repos: vec!["repo1".to_string(), "repo2".to_string()],
                expire_at: 12345,
            },
            RemoteOwner {
                remote: Cow::Owned("gitlab".to_string()),
                owner: Cow::Owned("bob".to_string()),
                repos: vec!["project1".to_string()],
                expire_at: 6789,
            },
        ]
    }

    fn build_conn() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        let tx = conn.transaction().unwrap();
        ensure_table(&tx).unwrap();
        let owners = test_remote_owners();
        for owner in owners {
            insert(&tx, &owner).unwrap();
        }
        tx.commit().unwrap();
        conn
    }

    #[test]
    fn test_insert() {
        let mut conn = build_conn();
        let tx = conn.transaction().unwrap();
        let owners = test_remote_owners();
        for owner in owners {
            let result = get_optional(&tx, &owner.remote, &owner.owner).unwrap();
            assert_eq!(result, Some(owner));
        }
    }

    #[test]
    fn test_get_not_found() {
        let mut conn = build_conn();
        let tx = conn.transaction().unwrap();
        let owner = get_optional(&tx, "unknown", "nobody").unwrap();
        assert_eq!(owner, None);
    }

    #[test]
    fn test_delete() {
        let mut conn = build_conn();
        let owners = test_remote_owners();
        for owner in owners {
            let tx = conn.transaction().unwrap();
            delete(&tx, &owner).unwrap();
            tx.commit().unwrap();
            let tx = conn.transaction().unwrap();
            let owner = get_optional(&tx, &owner.remote, &owner.owner).unwrap();
            assert_eq!(owner, None);
        }
    }
}

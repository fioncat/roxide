use std::borrow::Cow;
use std::path::{Path, PathBuf};

use anyhow::Result;

use rusqlite::{OptionalExtension, Row, Transaction, params};
use serde::{Deserialize, Serialize};

use crate::db::repo::Repository;
use crate::debug;
use crate::format::{format_time, now};
use crate::term::list::ListItem;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Mirror {
    pub id: u64,

    pub repo_id: u64,

    pub remote: String,

    pub owner: String,

    pub name: String,

    pub last_visited_at: u64,

    pub visited_count: u32,

    pub new_created: bool,
}

impl Mirror {
    pub fn new_from_repo(repo: &Repository, name: Option<String>) -> Self {
        let name = name.unwrap_or(format!("{}-mirror", repo.name));
        Self {
            repo_id: repo.id,
            remote: repo.remote.clone(),
            owner: repo.owner.clone(),
            name,
            new_created: true,
            ..Default::default()
        }
    }

    pub fn get_path<P: AsRef<Path>>(&self, mirrors_dir: P) -> PathBuf {
        mirrors_dir
            .as_ref()
            .join(Repository::escaped_path(&self.remote).as_ref())
            .join(Repository::escaped_path(&self.owner).as_ref())
            .join(&self.name)
    }

    pub fn into_repo(self) -> Repository {
        Repository {
            id: self.id,
            remote: self.remote,
            owner: self.owner,
            name: self.name,
            path: None,
            last_visited_at: self.last_visited_at,
            visited_count: self.visited_count,
            ..Default::default()
        }
    }

    #[inline]
    pub fn visit(&mut self) {
        self.last_visited_at = now();
        self.visited_count += 1;
    }

    fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            repo_id: row.get(1)?,
            remote: row.get(2)?,
            owner: row.get(3)?,
            name: row.get(4)?,
            last_visited_at: row.get(5)?,
            visited_count: row.get(6)?,
            new_created: false,
        })
    }
}

impl ListItem for Mirror {
    fn row<'a>(&'a self, title: &str) -> Cow<'a, str> {
        match title {
            "ID" => self.id.to_string().into(),
            "RepoID" => self.repo_id.to_string().into(),
            "Remote" => Cow::Borrowed(&self.remote),
            "Owner" => Cow::Borrowed(&self.owner),
            "Name" => Cow::Borrowed(&self.name),
            "LastVisited" => format_time(self.last_visited_at).into(),
            "Visited" => self.visited_count.to_string().into(),
            _ => Cow::Borrowed(""),
        }
    }
}

pub struct MirrorHandle<'a> {
    tx: &'a Transaction<'a>,
}

impl<'a> MirrorHandle<'a> {
    pub fn new(tx: &'a Transaction) -> Self {
        Self { tx }
    }

    pub fn ensure_table(&self) -> Result<()> {
        ensure_table(self.tx)
    }

    pub fn insert(&self, mirror: &Mirror) -> Result<u64> {
        insert(self.tx, mirror)
    }

    pub fn get(&self, remote: &str, owner: &str, name: &str) -> Result<Option<Mirror>> {
        get(self.tx, remote, owner, name)
    }

    pub fn update(&self, mirror: &Mirror) -> Result<()> {
        update(self.tx, mirror)
    }

    pub fn delete(&self, id: u64) -> Result<()> {
        delete(self.tx, id)
    }

    pub fn delete_by_repo_id(&self, repo_id: u64) -> Result<()> {
        delete_by_repo_id(self.tx, repo_id)
    }

    pub fn query_by_repo_id(&self, repo_id: u64) -> Result<Vec<Mirror>> {
        query_by_repo_id(self.tx, repo_id)
    }
}

const TABLE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS mirror (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    repo_id INTEGER NOT NULL,
    remote TEXT NOT NULL,
    owner TEXT NOT NULL,
    name TEXT NOT NULL,
    last_visited_at INTEGER NOT NULL,
    visited_count INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_mirror_repo_id ON mirror (repo_id);
CREATE UNIQUE INDEX IF NOT EXISTS idx_mirror_full ON mirror (remote, owner, name);
CREATE INDEX IF NOT EXISTS idx_mirror_last_visited_at ON mirror (last_visited_at DESC);
"#;

fn ensure_table(tx: &Transaction) -> Result<()> {
    debug!("[db] Ensure mirror table exists");
    tx.execute_batch(TABLE_SQL)?;
    Ok(())
}

const INSERT_SQL: &str = r#"
INSERT INTO mirror (
    repo_id,
    remote,
    owner,
    name,
    last_visited_at,
    visited_count
) VALUES (
    ?1, ?2, ?3, ?4, ?5, ?6
)
"#;

fn insert(tx: &Transaction, mirror: &Mirror) -> Result<u64> {
    debug!("[db] Insert mirror: {mirror:?}");
    tx.execute(
        INSERT_SQL,
        params![
            mirror.repo_id,
            mirror.remote,
            mirror.owner,
            mirror.name,
            mirror.last_visited_at,
            mirror.visited_count,
        ],
    )?;
    let id = tx.last_insert_rowid() as u64;
    debug!("[db] Inserted mirror id: {id}");
    Ok(id)
}

const GET_SQL: &str = r#"
SELECT id, repo_id, remote, owner, name, last_visited_at, visited_count
FROM mirror
WHERE remote = ?1 AND owner = ?2 AND name = ?3
"#;

fn get(tx: &Transaction, remote: &str, owner: &str, name: &str) -> Result<Option<Mirror>> {
    debug!("[db] Get mirror by full name: {remote}/{owner}/{name}");
    let mirror = tx
        .query_row(GET_SQL, params![remote, owner, name], Mirror::from_row)
        .optional()?;
    debug!("[db] Result: {mirror:?}");
    Ok(mirror)
}

const UPDATE_SQL: &str = r#"
UPDATE mirror SET last_visited_at = ?2, visited_count = ?3
WHERE id = ?1
"#;

fn update(tx: &Transaction, mirror: &Mirror) -> Result<()> {
    debug!("[db] Update mirror: {mirror:?}");
    tx.execute(
        UPDATE_SQL,
        params![mirror.id, mirror.last_visited_at, mirror.visited_count,],
    )?;
    Ok(())
}

const DELETE_SQL: &str = r#"
DELETE FROM mirror WHERE id = ?1
"#;

fn delete(tx: &Transaction, id: u64) -> Result<()> {
    debug!("[db] Delete mirror: {id}");
    tx.execute(DELETE_SQL, params![id])?;
    Ok(())
}

const DELETE_BY_REPO_ID_SQL: &str = r#"
DELETE FROM mirror WHERE repo_id = ?1
"#;

fn delete_by_repo_id(tx: &Transaction, repo_id: u64) -> Result<()> {
    debug!("[db] Delete mirror by repo_id: {repo_id}");
    tx.execute(DELETE_BY_REPO_ID_SQL, params![repo_id])?;
    Ok(())
}

const QUERY_BY_REPO_ID_SQL: &str = r#"
SELECT id, repo_id, remote, owner, name, last_visited_at, visited_count
FROM mirror
WHERE repo_id = ?1
ORDER BY last_visited_at DESC
"#;

fn query_by_repo_id(tx: &Transaction, repo_id: u64) -> Result<Vec<Mirror>> {
    debug!("[db] Query mirrors by repo_id: {repo_id}");
    let mut stmt = tx.prepare(QUERY_BY_REPO_ID_SQL)?;
    let rows = stmt.query_map(params![repo_id], Mirror::from_row)?;
    let mut results = Vec::new();
    for row in rows {
        results.push(row?);
    }

    debug!("[db] Query results: {results:?}");
    Ok(results)
}

#[cfg(test)]
pub mod tests {
    use rusqlite::Connection;

    use super::*;

    pub fn test_mirrors() -> Vec<Mirror> {
        vec![
            Mirror {
                id: 1,
                remote: "github".to_string(),
                owner: "fioncat".to_string(),
                name: "roxide-mirror".to_string(),
                repo_id: 1,
                last_visited_at: 1234,
                visited_count: 12,
                new_created: false,
            },
            Mirror {
                id: 2,
                remote: "github".to_string(),
                owner: "fioncat".to_string(),
                name: "roxide-golang".to_string(),
                repo_id: 1,
                last_visited_at: 3456,
                visited_count: 1,
                new_created: false,
            },
            Mirror {
                id: 3,
                remote: "github".to_string(),
                owner: "fioncat".to_string(),
                name: "roxide-rs".to_string(),
                repo_id: 1,
                last_visited_at: 89,
                visited_count: 3,
                new_created: false,
            },
        ]
    }

    fn build_conn() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        let tx = conn.transaction().unwrap();
        ensure_table(&tx).unwrap();
        let mirrors = test_mirrors();
        for mirror in mirrors {
            let id = insert(&tx, &mirror).unwrap();
            assert_eq!(id, mirror.id);
        }
        tx.commit().unwrap();
        conn
    }

    #[test]
    fn test_insert() {
        let mut conn = build_conn();
        let tx = conn.transaction().unwrap();
        let mirrors = test_mirrors();
        for mirror in mirrors {
            let result = get(&tx, &mirror.remote, &mirror.owner, &mirror.name)
                .unwrap()
                .unwrap();
            assert_eq!(result, mirror);
        }
    }

    #[test]
    fn test_get_not_found() {
        let mut conn = build_conn();
        let tx = conn.transaction().unwrap();
        let result = get(&tx, "github", "fioncat", "nonexist").unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_update() {
        let mut conn = build_conn();
        let tx = conn.transaction().unwrap();
        let mut mirror = get(&tx, "github", "fioncat", "roxide-mirror")
            .unwrap()
            .unwrap();
        mirror.last_visited_at = 9999;
        mirror.visited_count = 100;
        update(&tx, &mirror).unwrap();
        tx.commit().unwrap();

        let tx = conn.transaction().unwrap();
        let result = get(&tx, "github", "fioncat", "roxide-mirror")
            .unwrap()
            .unwrap();
        assert_eq!(result, mirror);
    }

    #[test]
    fn test_delete() {
        let mut conn = build_conn();
        let tx = conn.transaction().unwrap();
        let mirror = get(&tx, "github", "fioncat", "roxide-mirror")
            .unwrap()
            .unwrap();
        delete(&tx, mirror.id).unwrap();
        tx.commit().unwrap();

        let tx = conn.transaction().unwrap();
        let result = get(&tx, "github", "fioncat", "roxide-mirror").unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_delete_by_repo_id() {
        let mut conn = build_conn();
        let tx = conn.transaction().unwrap();
        delete_by_repo_id(&tx, 1).unwrap();
        tx.commit().unwrap();
        let tx = conn.transaction().unwrap();
        let mirrors = test_mirrors();
        for mirror in mirrors {
            let result = get(&tx, &mirror.remote, &mirror.owner, &mirror.name).unwrap();
            assert_eq!(result, None);
        }
    }

    #[test]
    fn test_query_by_repo_id() {
        let mut conn = build_conn();
        let tx = conn.transaction().unwrap();
        let results = query_by_repo_id(&tx, 1).unwrap();
        let expect = {
            let mut v = test_mirrors();
            v.sort_by(|a, b| b.last_visited_at.cmp(&a.last_visited_at));
            v
        };
        assert_eq!(results, expect);
    }
}

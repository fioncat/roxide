use std::borrow::Cow;
use std::fmt::Debug;

use anyhow::Result;
use rusqlite::{OptionalExtension, Row, Transaction, params};
use serde::{Deserialize, Serialize};

use crate::debug;
use crate::format::format_time;
use crate::term::list::ListItem;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct HookHistory {
    pub id: u64,

    pub repo_id: u64,

    pub name: String,

    pub success: bool,

    pub time: u64,
}

impl HookHistory {
    fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            repo_id: row.get(1)?,
            name: row.get(2)?,
            success: row.get(3)?,
            time: row.get(4)?,
        })
    }
}

impl ListItem for HookHistory {
    fn row<'a>(&'a self, title: &str) -> Cow<'a, str> {
        match title {
            "RepoID" => format!("{}", self.repo_id).into(),
            "Name" => Cow::Borrowed(&self.name),
            "Success" => {
                if self.success {
                    Cow::Borrowed("Yes")
                } else {
                    Cow::Borrowed("No")
                }
            }
            "Time" => format_time(self.time).into(),
            _ => Cow::Borrowed(""),
        }
    }
}

pub struct HookHistoryHandle<'a> {
    tx: &'a Transaction<'a>,
}

impl<'a> HookHistoryHandle<'a> {
    pub fn new(tx: &'a Transaction) -> Self {
        Self { tx }
    }

    pub fn ensure_table(&self) -> Result<()> {
        ensure_table(self.tx)
    }

    pub fn insert(&self, history: &HookHistory) -> Result<u64> {
        insert(self.tx, history)
    }

    pub fn get(&self, repo_id: u64, name: &str) -> Result<Option<HookHistory>> {
        get(self.tx, repo_id, name)
    }

    pub fn update(&self, history: &HookHistory) -> Result<()> {
        update(self.tx, history)
    }

    pub fn delete(&self, id: u64) -> Result<()> {
        delete(self.tx, id)
    }

    pub fn delete_by_repo_id(&self, repo_id: u64) -> Result<()> {
        delete_by_repo_id(self.tx, repo_id)
    }

    pub fn query_all(&self) -> Result<Vec<HookHistory>> {
        query_all(self.tx)
    }

    pub fn query_by_repo_id(&self, repo_id: u64) -> Result<Vec<HookHistory>> {
        query_by_repo_id(self.tx, repo_id)
    }
}

const TABLE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS hook_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    repo_id INTEGER NOT NULL,
    name TEXT NOT NULL,
    success INTEGER NOT NULL,
    time INTEGER NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_hook_history_repo_id_name
ON hook_history (repo_id, name);
"#;

fn ensure_table(tx: &Transaction) -> Result<()> {
    debug!("[db] Ensuring hook_history table exists");
    tx.execute_batch(TABLE_SQL)?;
    Ok(())
}

const INSERT_SQL: &str = r#"
INSERT INTO hook_history (
    repo_id,
    name,
    success,
    time
) VALUES (?1, ?2, ?3, ?4)
"#;

fn insert(tx: &Transaction, history: &HookHistory) -> Result<u64> {
    debug!("[db] Inserting hook_history: {history:?}");
    tx.execute(
        INSERT_SQL,
        params![history.repo_id, history.name, history.success, history.time,],
    )?;
    let id = tx.last_insert_rowid() as u64;
    debug!("[db] Inserted hook_history id: {id}");
    Ok(id)
}

const GET_SQL: &str = r#"
SELECT id, repo_id, name, success, time
FROM hook_history
WHERE repo_id = ?1 AND name = ?2
"#;

fn get(tx: &Transaction, repo_id: u64, name: &str) -> Result<Option<HookHistory>> {
    debug!("[db] Getting hook_history: repo_id={repo_id}, name={name}");
    let history = tx
        .query_row(GET_SQL, params![repo_id, name], HookHistory::from_row)
        .optional()?;
    debug!("[db] Result: {history:?}");
    Ok(history)
}

const UPDATE_SQL: &str = r#"
UPDATE hook_history
SET success = ?2, time = ?3
WHERE id = ?1
"#;

fn update(tx: &Transaction, history: &HookHistory) -> Result<()> {
    debug!("[db] Updating hook_history: {history:?}");
    tx.execute(
        UPDATE_SQL,
        params![history.id, history.success, history.time],
    )?;
    Ok(())
}

const DELETE_SQL: &str = r#"
DELETE FROM hook_history
WHERE id = ?1
"#;

fn delete(tx: &Transaction, id: u64) -> Result<()> {
    debug!("[db] Deleting hook_history: {id}");
    tx.execute(DELETE_SQL, params![id])?;
    Ok(())
}

const DELETE_BY_REPO_ID_SQL: &str = r#"
DELETE FROM hook_history
WHERE repo_id = ?1
"#;

fn delete_by_repo_id(tx: &Transaction, repo_id: u64) -> Result<()> {
    debug!("[db] Deleting hook_history by repo_id: {repo_id}");
    tx.execute(DELETE_BY_REPO_ID_SQL, params![repo_id])?;
    Ok(())
}

const QUERY_ALL_SQL: &str = r#"
SELECT id, repo_id, name, success, time
FROM hook_history
ORDER BY time DESC
"#;

fn query_all(tx: &Transaction) -> Result<Vec<HookHistory>> {
    debug!("[db] Querying all hook_history");
    let mut stmt = tx.prepare(QUERY_ALL_SQL)?;
    let rows = stmt.query_map([], HookHistory::from_row)?;
    let mut results = Vec::new();
    for row in rows {
        results.push(row?);
    }
    debug!("[db] Query results: {results:?}");
    Ok(results)
}

const QUERY_BY_REPO_ID_SQL: &str = r#"
SELECT id, repo_id, name, success, time
FROM hook_history
WHERE repo_id = ?1
ORDER BY time DESC
"#;

fn query_by_repo_id(tx: &Transaction, repo_id: u64) -> Result<Vec<HookHistory>> {
    debug!("[db] Querying hook_history by repo_id: {repo_id}");
    let mut stmt = tx.prepare(QUERY_BY_REPO_ID_SQL)?;
    let rows = stmt.query_map(params![repo_id], HookHistory::from_row)?;
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

    pub fn test_hook_histories() -> Vec<HookHistory> {
        vec![
            HookHistory {
                id: 1,
                repo_id: 1,
                name: "spell-check".to_string(),
                success: true,
                time: 100,
            },
            HookHistory {
                id: 2,
                repo_id: 1,
                name: "cargo-check".to_string(),
                success: false,
                time: 120,
            },
            HookHistory {
                id: 3,
                repo_id: 4,
                name: "cargo-init".to_string(),
                success: true,
                time: 7000,
            },
        ]
    }

    fn build_conn() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        let tx = conn.transaction().unwrap();
        ensure_table(&tx).unwrap();
        let histories = test_hook_histories();
        for history in histories {
            let id = insert(&tx, &history).unwrap();
            assert_eq!(id, history.id);
        }
        tx.commit().unwrap();
        conn
    }

    #[test]
    fn test_insert() {
        let mut conn = build_conn();
        let tx = conn.transaction().unwrap();
        let histories = test_hook_histories();
        for history in histories {
            let result = get(&tx, history.repo_id, &history.name).unwrap().unwrap();
            assert_eq!(result, history);
        }
    }

    #[test]
    fn test_get_not_found() {
        let mut conn = build_conn();
        let tx = conn.transaction().unwrap();
        let result = get(&tx, 999, "non-existent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_update() {
        let mut conn = build_conn();
        let tx = conn.transaction().unwrap();
        let mut history = get(&tx, 1, "spell-check").unwrap().unwrap();
        history.success = false;
        history.time = 200;
        update(&tx, &history).unwrap();
        let updated_history = get(&tx, 1, "spell-check").unwrap().unwrap();
        assert_eq!(updated_history, history);
    }

    #[test]
    fn test_delete() {
        let mut conn = build_conn();
        let tx = conn.transaction().unwrap();
        let history = get(&tx, 1, "spell-check").unwrap().unwrap();
        delete(&tx, history.id).unwrap();
        let result = get(&tx, 1, "spell-check").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_delete_by_repo_id() {
        let mut conn = build_conn();
        let tx = conn.transaction().unwrap();
        delete_by_repo_id(&tx, 1).unwrap();
        let result1 = get(&tx, 1, "spell-check").unwrap();
        assert!(result1.is_none());
        let result2 = get(&tx, 1, "cargo-check").unwrap();
        assert!(result2.is_none());
        let result3 = get(&tx, 4, "cargo-init").unwrap();
        assert!(result3.is_some());
    }

    #[test]
    fn test_query_all() {
        let mut conn = build_conn();
        let tx = conn.transaction().unwrap();
        let results = query_all(&tx).unwrap();
        let mut expects = test_hook_histories();
        expects.sort_by(|a, b| b.time.cmp(&a.time));
        assert_eq!(results, expects);
    }

    #[test]
    fn test_query_by_repo_id() {
        let mut conn = build_conn();
        let tx = conn.transaction().unwrap();
        let results = query_by_repo_id(&tx, 1).unwrap();
        let mut expects = test_hook_histories()
            .into_iter()
            .filter(|h| h.repo_id == 1)
            .collect::<Vec<_>>();
        expects.sort_by(|a, b| b.time.cmp(&a.time));
        assert_eq!(results, expects);
    }
}

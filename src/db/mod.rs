pub mod hook_history;
pub mod mirror;
pub mod remote_owner;
pub mod remote_repo;
pub mod repo;

use std::path::Path;
use std::sync::Mutex;

use anyhow::{Result, bail};
use rusqlite::{Connection, Transaction};

pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let conn = Connection::open(path)?;
        let db = Self {
            conn: Mutex::new(conn),
        };
        db.ensure_tables()?;
        Ok(db)
    }

    fn ensure_tables(&self) -> Result<()> {
        self.with_transaction(|tx| {
            tx.repo().ensure_table()?;
            tx.mirror().ensure_table()?;
            tx.hook_history().ensure_table()?;
            tx.remote_owner().ensure_table()?;
            tx.remote_repo().ensure_table()?;
            Ok(())
        })
    }

    pub fn with_transaction<T, F>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&DatabaseHandle) -> Result<T>,
    {
        let mut conn = match self.conn.lock() {
            Ok(conn) => conn,
            Err(e) => bail!("failed to lock connection: {:#}", e),
        };
        let tx = conn.transaction()?;
        let handle = DatabaseHandle { tx };

        let result = f(&handle);

        if result.is_ok() {
            handle.tx.commit()
        } else {
            handle.tx.rollback()
        }?;

        result
    }
}

pub struct DatabaseHandle<'a> {
    tx: Transaction<'a>,
}

impl DatabaseHandle<'_> {
    pub fn repo<'a>(&'a self) -> repo::RepositoryHandle<'a> {
        repo::RepositoryHandle::new(&self.tx)
    }

    pub fn mirror<'a>(&'a self) -> mirror::MirrorHandle<'a> {
        mirror::MirrorHandle::new(&self.tx)
    }

    pub fn hook_history<'a>(&'a self) -> hook_history::HookHistoryHandle<'a> {
        hook_history::HookHistoryHandle::new(&self.tx)
    }

    pub fn remote_owner<'a>(&'a self) -> remote_owner::RemoteOwnerHandle<'a> {
        remote_owner::RemoteOwnerHandle::new(&self.tx)
    }

    pub fn remote_repo<'a>(&'a self) -> remote_repo::RemoteRepositoryHandle<'a> {
        remote_repo::RemoteRepositoryHandle::new(&self.tx)
    }
}

#[cfg(test)]
pub mod tests {
    use std::borrow::Cow;
    use std::fs;

    use anyhow::bail;

    use super::*;

    pub use super::mirror::tests::test_mirrors;
    pub use super::repo::tests::test_repos;

    fn build_db(name: &str) -> Database {
        // remove the existing file
        let _ = fs::remove_file(name);
        Database::open(name).unwrap()
    }

    #[test]
    fn test_commit() {
        let repo = repo::Repository {
            id: 1,
            remote: "github".to_string(),
            owner: "fioncat".to_string(),
            name: "roxide".to_string(),
            path: None,
            sync: true,
            pin: true,
            last_visited_at: 2234,
            visited_count: 20,
            new_created: false,
        };
        let db = build_db("tests/commit.db");
        db.with_transaction(|tx| {
            tx.repo().insert(&repo)?;
            Ok(())
        })
        .unwrap();

        let result = db
            .with_transaction(|tx| tx.repo().get("github", "fioncat", "roxide"))
            .unwrap()
            .unwrap();
        assert_eq!(result, repo);
    }

    #[test]
    fn test_rollback() {
        let db = build_db("tests/rollback.db");
        let owner = remote_owner::RemoteOwner {
            remote: Cow::Owned("github".to_string()),
            owner: Cow::Owned("alice".to_string()),
            repos: vec!["repo1".to_string(), "repo2".to_string()],
            expire_at: 12345,
        };
        let result: Result<()> = db.with_transaction(|tx| {
            tx.remote_owner().insert(&owner).unwrap();
            bail!("force rollback");
        });
        assert_eq!(result.err().unwrap().to_string(), "force rollback");

        let result = db
            .with_transaction(|tx| tx.remote_owner().get_optional("github", "alice"))
            .unwrap();
        assert_eq!(result, None);
    }
}

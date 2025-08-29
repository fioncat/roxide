pub mod remote_owner;
pub mod remote_repo;
pub mod repo;

use std::path::Path;

use anyhow::Result;
use rusqlite::Connection;

#[allow(dead_code)] // TODO: remove this
pub struct Database {
    conn: Connection,
}

#[allow(dead_code)] // TODO: remove this
impl Database {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let conn = Connection::open(path)?;
        Ok(Self { conn })
    }

    pub fn repo<'a>(&'a self) -> repo::RepositoryHandle<'a> {
        repo::RepositoryHandle::new(&self.conn)
    }

    pub fn remote_owner<'a>(&'a self) -> remote_owner::RemoteOwnerHandle<'a> {
        remote_owner::RemoteOwnerHandle::new(&self.conn)
    }

    pub fn remote_repo<'a>(&'a self) -> remote_repo::RemoteRepositoryHandle<'a> {
        remote_repo::RemoteRepositoryHandle::new(&self.conn)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LimitOptions {
    pub offset: u32,
    pub limit: u32,
}

impl Default for LimitOptions {
    fn default() -> Self {
        Self {
            offset: 0,
            limit: 100,
        }
    }
}

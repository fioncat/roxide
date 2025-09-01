use std::borrow::Cow;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use rusqlite::{OptionalExtension, Row, Transaction, params};
use serde::{Deserialize, Serialize};

use crate::debug;
use crate::format::format_time;
use crate::term::list::ListItem;

use super::LimitOptions;

/// The database model for a repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Repository {
    /// Remote name, e.g. "github"
    pub remote: String,
    /// Owner name, e.g. "fioncat"
    pub owner: String,
    /// Repository name, e.g. "roxide"
    pub name: String,

    /// This is optional, if none, the repository is in the workspace.
    pub path: Option<String>,

    /// The last time the repository was visited, as a unix timestamp.
    pub last_visited_at: u64,

    /// The number of times the repository has been visited.
    pub visited_count: u32,
}

impl Repository {
    pub fn get_path<P: AsRef<Path>>(&self, workspace: P) -> PathBuf {
        if let Some(ref path) = self.path {
            return PathBuf::from(path);
        }

        PathBuf::from(workspace.as_ref())
            .join(Self::escaped_path(&self.remote).as_ref())
            .join(Self::escaped_path(&self.owner).as_ref())
            .join(Self::escaped_path(&self.name).as_ref())
    }

    #[inline]
    pub fn visit_at(&mut self, now: u64) {
        self.last_visited_at = now;
        self.visited_count += 1;
    }

    fn escaped_path<'a>(raw: &'a str) -> Cow<'a, str> {
        use std::path::MAIN_SEPARATOR;

        if !raw.contains(MAIN_SEPARATOR) {
            return Cow::Borrowed(raw);
        }
        Cow::Owned(raw.replace(MAIN_SEPARATOR, "."))
    }

    fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(Self {
            remote: row.get(0)?,
            owner: row.get(1)?,
            name: row.get(2)?,
            path: row.get(3)?,
            last_visited_at: row.get(4)?,
            visited_count: row.get(5)?,
        })
    }
}

impl ListItem for Repository {
    fn titles() -> Vec<&'static str> {
        vec!["Remote", "Owner", "Name", "LastVisited", "Visited"]
    }

    fn row(self) -> Vec<String> {
        vec![
            self.remote,
            self.owner,
            self.name,
            format!("{}", format_time(self.last_visited_at)),
            format!("{}", self.visited_count),
        ]
    }
}

/// Database handler for repository.
pub struct RepositoryHandle<'a> {
    tx: &'a Transaction<'a>,
}

impl<'a> RepositoryHandle<'a> {
    /// Create a new repository handle.
    pub fn new(tx: &'a Transaction) -> Self {
        Self { tx }
    }

    /// Ensure the repository table exists.
    /// This should be called before any other operations.
    pub fn ensure_table(&self) -> Result<()> {
        ensure_table(self.tx)
    }

    /// Insert a new repository.
    pub fn insert(&self, repo: &Repository) -> Result<()> {
        insert(self.tx, repo)
    }

    /// Get a repository by remote, owner and name.
    pub fn get_optional(
        &self,
        remote: &str,
        owner: &str,
        name: &str,
    ) -> Result<Option<Repository>> {
        get_optional(self.tx, remote, owner, name)
    }

    /// Same as `get_optional`, but returns an error if not found.
    pub fn get(&self, remote: &str, owner: &str, name: &str) -> Result<Repository> {
        get(self.tx, remote, owner, name)
    }

    /// Get a repository by path.
    pub fn get_by_path_optional(&self, path: &str) -> Result<Option<Repository>> {
        get_by_path_optional(self.tx, path)
    }

    /// Same as `get_by_path_optional`, but returns an error if not found.
    pub fn get_by_path(&self, path: &str) -> Result<Repository> {
        get_by_path(self.tx, path)
    }

    /// Update a repository's `last_visited_at` and `visited_count`.
    pub fn update(&self, repo: &Repository) -> Result<()> {
        update(self.tx, repo)
    }

    /// Delete a repository.
    pub fn delete(&self, repo: &Repository) -> Result<()> {
        delete(self.tx, repo)
    }

    /// Query all repositories, ordered by `last_visited_at` desc.
    pub fn query_all(&self, limit: LimitOptions) -> Result<Vec<Repository>> {
        query_all(self.tx, limit)
    }

    /// Count all repositories.
    pub fn count_all(&self) -> Result<u32> {
        count_all(self.tx)
    }

    /// Query repositories by remote, ordered by `last_visited_at` desc.
    pub fn query_by_remote(&self, remote: &str, limit: LimitOptions) -> Result<Vec<Repository>> {
        query_by_remote(self.tx, remote, limit)
    }

    /// Count repositories by remote.
    pub fn count_by_remote(&self, remote: &str) -> Result<u32> {
        count_by_remote(self.tx, remote)
    }

    /// Query repositories by remote and owner, ordered by `last_visited_at` desc.
    pub fn query_by_owner(
        &self,
        remote: &str,
        owner: &str,
        limit: LimitOptions,
    ) -> Result<Vec<Repository>> {
        query_by_owner(self.tx, remote, owner, limit)
    }

    /// Count repositories by remote and owner.
    pub fn count_by_owner(&self, remote: &str, owner: &str) -> Result<u32> {
        count_by_owner(self.tx, remote, owner)
    }

    /// Query distinct remotes with owner and repo counts, ordered by
    /// latest `last_visited_at` desc.
    pub fn query_remotes(&self, limit: LimitOptions) -> Result<Vec<RemoteState>> {
        query_remotes(self.tx, limit)
    }

    /// Count distinct remotes.
    pub fn count_remotes(&self) -> Result<u32> {
        count_remotes(self.tx)
    }

    /// Query distinct owners for a remote with repo counts, ordered by
    /// latest `last_visited_at` desc.
    pub fn query_owners(&self, remote: &str, limit: LimitOptions) -> Result<Vec<OwnerState>> {
        query_owners(self.tx, remote, limit)
    }

    /// Count distinct owners for a remote.
    pub fn count_owners(&self, remote: &str) -> Result<u32> {
        count_owners(self.tx, remote)
    }
}

const TABLE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS repo (
    remote TEXT NOT NULL,
    owner TEXT NOT NULL,
    name TEXT NOT NULL,
    path TEXT,
    last_visited_at INTEGER NOT NULL,
    visited_count INTEGER NOT NULL,
    PRIMARY KEY (remote, owner, name)
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_repo_unique_path
ON repo (path)
WHERE path IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_repo_remote ON repo (remote);
CREATE INDEX IF NOT EXISTS idx_repo_owner ON repo (remote, owner);
CREATE INDEX IF NOT EXISTS idx_repo_last_visited_at ON repo (last_visited_at DESC);
"#;

fn ensure_table(tx: &Transaction) -> Result<()> {
    debug!("[db] Ensure repo table exists");
    tx.execute_batch(TABLE_SQL)?;
    Ok(())
}

const INSERT_SQL: &str = r#"
INSERT INTO repo (
    remote,
    owner,
    name,
    path,
    last_visited_at,
    visited_count
) VALUES (
    ?1, ?2, ?3, ?4, ?5, ?6
)
"#;

fn insert(tx: &Transaction, repo: &Repository) -> Result<()> {
    debug!("[db] Insert repo: {repo:?}");
    tx.execute(
        INSERT_SQL,
        params![
            repo.remote,
            repo.owner,
            repo.name,
            repo.path,
            repo.last_visited_at,
            repo.visited_count,
        ],
    )?;
    Ok(())
}

const GET_SQL: &str = r#"
SELECT remote, owner, name, path, last_visited_at, visited_count
FROM repo
WHERE remote = ?1 AND owner = ?2 AND name = ?3
"#;

fn get_optional(
    tx: &Transaction,
    remote: &str,
    owner: &str,
    name: &str,
) -> Result<Option<Repository>> {
    debug!("[db] Get repo: {remote}:{owner}:{name}");
    let mut stmt = tx.prepare(GET_SQL)?;
    let repo = stmt
        .query_row(params![remote, owner, name], Repository::from_row)
        .optional()?;
    debug!("[db] Result: {repo:?}");
    Ok(repo)
}

fn get(tx: &Transaction, remote: &str, owner: &str, name: &str) -> Result<Repository> {
    let repo = get_optional(tx, remote, owner, name)?;
    match repo {
        Some(repo) => Ok(repo),
        None => bail!("repo not found"),
    }
}

const GET_BY_PATH_SQL: &str = r#"
SELECT remote, owner, name, path, last_visited_at, visited_count
FROM repo
WHERE path = ?1
"#;

fn get_by_path_optional(tx: &Transaction, path: &str) -> Result<Option<Repository>> {
    debug!("[db] Get repo by path: {path}");
    let mut stmt = tx.prepare(GET_BY_PATH_SQL)?;
    let repo = stmt.query_row([path], Repository::from_row).optional()?;
    debug!("[db] Result: {repo:?}");
    Ok(repo)
}

fn get_by_path(tx: &Transaction, path: &str) -> Result<Repository> {
    let repo = get_by_path_optional(tx, path)?;
    match repo {
        Some(repo) => Ok(repo),
        None => bail!("this path has not been attached to any repo"),
    }
}

const UPDATE_SQL: &str = r#"
UPDATE repo SET last_visited_at = ?4, visited_count = ?5
WHERE remote = ?1 AND owner = ?2 AND name = ?3
"#;

fn update(tx: &Transaction, repo: &Repository) -> Result<()> {
    debug!("[db] Update repo: {repo:?}");
    tx.execute(
        UPDATE_SQL,
        params![
            repo.remote,
            repo.owner,
            repo.name,
            repo.last_visited_at,
            repo.visited_count,
        ],
    )?;
    Ok(())
}

const DELETE_SQL: &str = r#"
DELETE FROM repo
WHERE remote = ?1 AND owner = ?2 AND name = ?3
"#;

fn delete(tx: &Transaction, repo: &Repository) -> Result<()> {
    debug!("[db] Delete repo: {repo:?}");
    tx.execute(DELETE_SQL, params![repo.remote, repo.owner, repo.name])?;
    Ok(())
}

const QUERY_ALL_SQL: &str = r#"
SELECT remote, owner, name, path, last_visited_at, visited_count
FROM repo
ORDER BY last_visited_at DESC
LIMIT ?2 OFFSET ?1
"#;

fn query_all(tx: &Transaction, limit: LimitOptions) -> Result<Vec<Repository>> {
    debug!("[db] Query all repos, limit: {limit:?}");
    let mut stmt = tx.prepare(QUERY_ALL_SQL)?;
    let rows = stmt.query_map(params![limit.offset, limit.limit], |row| {
        Repository::from_row(row)
    })?;
    let mut results = Vec::new();
    for row in rows {
        results.push(row?);
    }
    debug!("[db] Results: {results:?}");
    Ok(results)
}

const COUNT_ALL_SQL: &str = r#"
SELECT COUNT(*) FROM repo
"#;

fn count_all(tx: &Transaction) -> Result<u32> {
    debug!("[db] Count all repos");
    let count: u32 = tx.query_row(COUNT_ALL_SQL, [], |row| row.get(0))?;
    debug!("[db] Result: {count}");
    Ok(count)
}

const QUERY_BY_REMOTE_SQL: &str = r#"
SELECT remote, owner, name, path, last_visited_at, visited_count
FROM repo
WHERE remote = ?1
ORDER BY last_visited_at DESC
LIMIT ?3 OFFSET ?2
"#;

fn query_by_remote(tx: &Transaction, remote: &str, limit: LimitOptions) -> Result<Vec<Repository>> {
    debug!("[db] Query repos by remote: {remote}, limit: {limit:?}");
    let mut stmt = tx.prepare(QUERY_BY_REMOTE_SQL)?;
    let rows = stmt.query_map(params![remote, limit.offset, limit.limit], |row| {
        Repository::from_row(row)
    })?;
    let mut results = Vec::new();
    for row in rows {
        results.push(row?);
    }
    debug!("[db] Results: {results:?}");
    Ok(results)
}

const COUNT_BY_REMOTE_SQL: &str = r#"
SELECT COUNT(*) FROM repo WHERE remote = ?1
"#;

fn count_by_remote(tx: &Transaction, remote: &str) -> Result<u32> {
    debug!("[db] Count repos by remote: {remote}");
    let count: u32 = tx.query_row(COUNT_BY_REMOTE_SQL, [remote], |row| row.get(0))?;
    debug!("[db] Result: {count}");
    Ok(count)
}

const QUERY_BY_OWNER_SQL: &str = r#"
SELECT remote, owner, name, path, last_visited_at, visited_count
FROM repo
WHERE remote = ?1 AND owner = ?2
ORDER BY last_visited_at DESC
LIMIT ?4 OFFSET ?3
"#;

fn query_by_owner(
    tx: &Transaction,
    remote: &str,
    owner: &str,
    limit: LimitOptions,
) -> Result<Vec<Repository>> {
    debug!("[db] Query repos by remote: {remote}, owner: {owner}, limit: {limit:?}");
    let mut stmt = tx.prepare(QUERY_BY_OWNER_SQL)?;
    let rows = stmt.query_map(params![remote, owner, limit.offset, limit.limit], |row| {
        Repository::from_row(row)
    })?;
    let mut results = Vec::new();
    for row in rows {
        results.push(row?);
    }
    debug!("[db] Results: {results:?}");
    Ok(results)
}

const COUNT_BY_OWNER_SQL: &str = r#"
SELECT COUNT(*) FROM repo WHERE remote = ?1 AND owner = ?2
"#;

fn count_by_owner(tx: &Transaction, remote: &str, owner: &str) -> Result<u32> {
    debug!("[db] Count repos by remote: {remote}, owner: {owner}");
    let count: u32 = tx.query_row(COUNT_BY_OWNER_SQL, params![remote, owner], |row| row.get(0))?;
    debug!("[db] Result: {count}");
    Ok(count)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteState {
    pub remote: String,
    pub owner_count: u32,
    pub repo_count: u32,
}

const QUERY_REMOTES_SQL: &str = r#"
SELECT remote, COUNT(DISTINCT owner) AS owner_count, COUNT(*) AS repo_count
FROM repo
GROUP BY remote
ORDER BY MAX(last_visited_at) DESC
LIMIT ?2 OFFSET ?1
"#;

fn query_remotes(tx: &Transaction, limit: LimitOptions) -> Result<Vec<RemoteState>> {
    debug!("[db] Query remotes, limit: {limit:?}");
    let mut stmt = tx.prepare(QUERY_REMOTES_SQL)?;
    let rows = stmt.query_map([limit.offset, limit.limit], |row| {
        Ok(RemoteState {
            remote: row.get(0)?,
            owner_count: row.get(1)?,
            repo_count: row.get(2)?,
        })
    })?;
    let mut results = Vec::new();
    for row in rows {
        results.push(row?);
    }
    debug!("[db] Results: {results:?}");
    Ok(results)
}

const COUNT_REMOTES_SQL: &str = r#"
SELECT COUNT(DISTINCT remote) FROM repo
"#;

fn count_remotes(tx: &Transaction) -> Result<u32> {
    debug!("[db] Count remotes");
    let count: u32 = tx.query_row(COUNT_REMOTES_SQL, [], |row| row.get(0))?;
    debug!("[db] Result: {count}");
    Ok(count)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnerState {
    pub remote: String,
    pub owner: String,
    pub repo_count: u32,
}

const QUERY_OWNERS_SQL: &str = r#"
SELECT remote, owner, COUNT(*) AS repo_count
FROM repo
WHERE remote = ?1
GROUP BY remote, owner
ORDER BY MAX(last_visited_at) DESC
LIMIT ?3 OFFSET ?2
"#;

fn query_owners(tx: &Transaction, remote: &str, limit: LimitOptions) -> Result<Vec<OwnerState>> {
    debug!("[db] Query owners for remote: {remote}, limit: {limit:?}");
    let mut stmt = tx.prepare(QUERY_OWNERS_SQL)?;
    let rows = stmt.query_map(params![remote, limit.offset, limit.limit], |row| {
        Ok(OwnerState {
            remote: row.get(0)?,
            owner: row.get(1)?,
            repo_count: row.get(2)?,
        })
    })?;
    let mut results = Vec::new();
    for row in rows {
        results.push(row?);
    }
    debug!("[db] Results: {results:?}");
    Ok(results)
}

const COUNT_OWNERS_SQL: &str = r#"
SELECT COUNT(DISTINCT owner) FROM repo WHERE remote = ?1
"#;

fn count_owners(tx: &Transaction, remote: &str) -> Result<u32> {
    debug!("[db] Count owners for remote: {remote}");
    let count: u32 = tx.query_row(COUNT_OWNERS_SQL, [remote], |row| row.get(0))?;
    debug!("[db] Result: {count}");
    Ok(count)
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use super::*;

    #[test]
    fn test_get_path() {
        let test_cases = vec![
            (
                "github",
                "fioncat",
                "roxide",
                None,
                "/dev/github/fioncat/roxide",
            ),
            (
                "github",
                "fioncat",
                "roxide",
                Some("/myproject"),
                "/myproject",
            ),
            (
                "gitlab",
                "some/one",
                "otree",
                None,
                "/dev/gitlab/some.one/otree",
            ),
        ];
        for case in test_cases {
            let repo = Repository {
                remote: case.0.to_string(),
                owner: case.1.to_string(),
                name: case.2.to_string(),
                path: case.3.map(String::from),
                last_visited_at: 0,
                visited_count: 0,
            };
            let path = repo.get_path("/dev");
            assert_eq!(path, PathBuf::from(case.4));
        }
    }

    fn build_conn() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        let tx = conn.transaction().unwrap();
        ensure_table(&tx).unwrap();
        let repos = test_repos();
        for repo in repos {
            insert(&tx, &repo).unwrap();
        }
        tx.commit().unwrap();
        conn
    }

    fn test_repos() -> Vec<Repository> {
        vec![
            Repository {
                remote: "github".to_string(),
                owner: "fioncat".to_string(),
                name: "roxide".to_string(),
                path: None,
                last_visited_at: 2234,
                visited_count: 20,
            },
            Repository {
                remote: "github".to_string(),
                owner: "fioncat".to_string(),
                name: "otree".to_string(),
                path: None,
                last_visited_at: 0,
                visited_count: 0,
            },
            Repository {
                remote: "github".to_string(),
                owner: "fioncat".to_string(),
                name: "nvimdots".to_string(),
                path: Some("/path/to/nvimdots".to_string()),
                last_visited_at: 3333,
                visited_count: 0,
            },
            Repository {
                remote: "github".to_string(),
                owner: "kubernetes".to_string(),
                name: "kubernetes".to_string(),
                path: None,
                last_visited_at: 7777,
                visited_count: 100,
            },
            Repository {
                remote: "gitlab".to_string(),
                owner: "fioncat".to_string(),
                name: "someproject".to_string(),
                path: None,
                last_visited_at: 1234,
                visited_count: 10,
            },
            Repository {
                remote: "gitlab".to_string(),
                owner: "fioncat".to_string(),
                name: "template".to_string(),
                path: None,
                last_visited_at: 2222,
                visited_count: 20,
            },
        ]
    }

    #[test]
    fn test_insert() {
        let mut conn = build_conn();
        let tx = conn.transaction().unwrap();
        let repos = test_repos();
        for repo in repos {
            let result = get(&tx, &repo.remote, &repo.owner, &repo.name).unwrap();
            assert_eq!(result, repo);
        }
    }

    #[test]
    fn test_get_not_found() {
        let mut conn = build_conn();
        let tx = conn.transaction().unwrap();
        let repo = get_optional(&tx, "github", "fioncat", "nonexist").unwrap();
        assert_eq!(repo, None);
    }

    #[test]
    fn test_update() {
        let mut conn = build_conn();
        let tx = conn.transaction().unwrap();
        let mut repo = get(&tx, "github", "fioncat", "roxide").unwrap();
        repo.last_visited_at = 9999;
        repo.visited_count = 42;
        update(&tx, &repo).unwrap();
        tx.commit().unwrap();

        let tx = conn.transaction().unwrap();
        let result = get(&tx, "github", "fioncat", "roxide").unwrap();
        assert_eq!(result, repo);
    }

    #[test]
    fn test_delete() {
        let mut conn = build_conn();
        let tx = conn.transaction().unwrap();
        let repo = get(&tx, "github", "fioncat", "roxide").unwrap();
        delete(&tx, &repo).unwrap();
        tx.commit().unwrap();

        let tx = conn.transaction().unwrap();
        let result = get(&tx, "github", "fioncat", "roxide");
        assert!(result.err().unwrap().to_string().contains("repo not found"));
    }

    #[test]
    fn test_get_by_path() {
        let mut conn = build_conn();
        let tx = conn.transaction().unwrap();
        let repo = get_by_path_optional(&tx, "/path/to/nvimdots").unwrap();
        let expect = get(&tx, "github", "fioncat", "nvimdots").unwrap();
        assert_eq!(repo, Some(expect));
        let repo = get_by_path_optional(&tx, "/path/to/nonexist").unwrap();
        assert_eq!(repo, None);
    }

    #[test]
    fn test_get_all() {
        let mut conn = build_conn();
        let tx = conn.transaction().unwrap();
        let repos = query_all(&tx, LimitOptions::default()).unwrap();
        let mut expects = test_repos();
        expects.sort_by(|a, b| b.last_visited_at.cmp(&a.last_visited_at));
        assert_eq!(repos, expects);

        let repos_limit = query_all(
            &tx,
            LimitOptions {
                offset: 1,
                limit: 2,
            },
        )
        .unwrap();
        assert_eq!(
            repos_limit,
            expects.drain(1..3).collect::<Vec<Repository>>()
        );
    }

    #[test]
    fn test_count_all() {
        let mut conn = build_conn();
        let tx = conn.transaction().unwrap();
        let count = count_all(&tx).unwrap();
        let repos = test_repos();
        assert_eq!(count, repos.len() as u32);
    }

    #[test]
    fn test_get_by_remote() {
        let mut conn = build_conn();
        let tx = conn.transaction().unwrap();
        let repos = query_by_remote(&tx, "github", LimitOptions::default()).unwrap();
        let mut expects: Vec<Repository> = test_repos()
            .into_iter()
            .filter(|r| r.remote == "github")
            .collect();
        expects.sort_by(|a, b| b.last_visited_at.cmp(&a.last_visited_at));
        assert_eq!(repos, expects);
        let repos_limit = query_by_remote(
            &tx,
            "github",
            LimitOptions {
                offset: 1,
                limit: 1,
            },
        )
        .unwrap();
        assert_eq!(
            repos_limit,
            expects.drain(1..2).collect::<Vec<Repository>>()
        );
    }

    #[test]
    fn test_count_by_remote() {
        let mut conn = build_conn();
        let tx = conn.transaction().unwrap();
        let count = count_by_remote(&tx, "github").unwrap();
        let repos = test_repos();
        let expect = repos.iter().filter(|r| r.remote == "github").count() as u32;
        assert_eq!(count, expect);
        let count = count_by_remote(&tx, "gitlab").unwrap();
        let expect = repos.iter().filter(|r| r.remote == "gitlab").count() as u32;
        assert_eq!(count, expect);
    }

    #[test]
    fn test_get_by_owner() {
        let mut conn = build_conn();
        let tx = conn.transaction().unwrap();
        let repos = query_by_owner(&tx, "github", "fioncat", LimitOptions::default()).unwrap();
        let mut expects: Vec<Repository> = test_repos()
            .into_iter()
            .filter(|r| r.remote == "github" && r.owner == "fioncat")
            .collect();
        expects.sort_by(|a, b| b.last_visited_at.cmp(&a.last_visited_at));
        assert_eq!(repos, expects);
        let repos_limit = query_by_owner(
            &tx,
            "github",
            "fioncat",
            LimitOptions {
                offset: 1,
                limit: 1,
            },
        )
        .unwrap();
        assert_eq!(
            repos_limit,
            expects.drain(1..2).collect::<Vec<Repository>>()
        );
    }

    #[test]
    fn test_count_by_owner() {
        let mut conn = build_conn();
        let tx = conn.transaction().unwrap();
        let count = count_by_owner(&tx, "github", "fioncat").unwrap();
        let repos = test_repos();
        let expect = repos
            .iter()
            .filter(|r| r.remote == "github" && r.owner == "fioncat")
            .count() as u32;
        assert_eq!(count, expect);
        let count = count_by_owner(&tx, "gitlab", "fioncat").unwrap();
        let expect = repos
            .iter()
            .filter(|r| r.remote == "gitlab" && r.owner == "fioncat")
            .count() as u32;
        assert_eq!(count, expect);
    }

    #[test]
    fn test_query_remotes() {
        let mut conn = build_conn();
        let tx = conn.transaction().unwrap();
        let results = query_remotes(&tx, LimitOptions::default()).unwrap();
        let expects = vec![
            RemoteState {
                remote: "github".to_string(),
                owner_count: 2,
                repo_count: 4,
            },
            RemoteState {
                remote: "gitlab".to_string(),
                owner_count: 1,
                repo_count: 2,
            },
        ];
        assert_eq!(results, expects);
    }

    #[test]
    fn test_count_remotes() {
        let mut conn = build_conn();
        let tx = conn.transaction().unwrap();
        let count = count_remotes(&tx).unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn test_query_owners() {
        let mut conn = build_conn();
        let tx = conn.transaction().unwrap();
        let results = query_owners(&tx, "github", LimitOptions::default()).unwrap();
        let expects = vec![
            OwnerState {
                remote: "github".to_string(),
                owner: "kubernetes".to_string(),
                repo_count: 1,
            },
            OwnerState {
                remote: "github".to_string(),
                owner: "fioncat".to_string(),
                repo_count: 3,
            },
        ];
        assert_eq!(results, expects);
    }

    #[test]
    fn test_count_owners() {
        let mut conn = build_conn();
        let tx = conn.transaction().unwrap();
        let count = count_owners(&tx, "github").unwrap();
        assert_eq!(count, 2);
        let count = count_owners(&tx, "gitlab").unwrap();
        assert_eq!(count, 1);
    }
}

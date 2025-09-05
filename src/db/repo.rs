use std::borrow::Cow;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use rusqlite::types::Value;
use rusqlite::{OptionalExtension, Row, Transaction, params, params_from_iter};
use serde::{Deserialize, Serialize};

use crate::debug;
use crate::format::format_time;
use crate::term::list::ListItem;

/// The database model for a repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Repository {
    /// Remote name, e.g. "github"
    pub remote: String,
    /// Owner name, e.g. "fioncat"
    pub owner: String,
    /// Repository name, e.g. "roxide"
    pub name: String,

    /// This is optional, if none, the repository is in the workspace.
    pub path: Option<String>,

    pub pin: bool,

    pub sync: bool,

    /// The last time the repository was visited, as a unix timestamp.
    pub last_visited_at: u64,

    /// The number of times the repository has been visited.
    pub visited_count: u32,

    pub new_created: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayLevel {
    Remote,
    Owner,
    Name,
}

impl Repository {
    pub fn get_path<P: AsRef<Path>>(&self, workspace: P) -> PathBuf {
        if let Some(ref path) = self.path {
            return PathBuf::from(path);
        }

        PathBuf::from(workspace.as_ref())
            .join(Self::escaped_path(&self.remote).as_ref())
            .join(Self::escaped_path(&self.owner).as_ref())
            .join(&self.name)
    }

    pub fn get_clone_url(&self, domain: &str, ssh: bool) -> String {
        Self::get_clone_url_raw(domain, &self.owner, &self.name, ssh)
    }

    pub fn get_clone_url_raw(domain: &str, owner: &str, name: &str, ssh: bool) -> String {
        if ssh {
            format!("git@{domain}:{owner}/{name}.git")
        } else {
            format!("https://{domain}/{owner}/{name}.git")
        }
    }

    pub fn search_item<'a>(&'a self, level: DisplayLevel) -> Cow<'a, str> {
        match level {
            DisplayLevel::Name => Cow::Borrowed(&self.name),
            DisplayLevel::Owner => {
                Cow::Owned(format!("{}/{}", Self::escaped_path(&self.owner), self.name))
            }
            DisplayLevel::Remote => Cow::Owned(format!(
                "{}/{}/{}",
                Self::escaped_path(&self.remote),
                Self::escaped_path(&self.owner),
                self.name
            )),
        }
    }

    pub fn full_name(&self) -> String {
        format!(
            "{}:{}/{}",
            Self::escaped_path(&self.remote),
            Self::escaped_path(&self.owner),
            self.name,
        )
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

    pub fn parse_escaped_path(path: &str) -> String {
        use std::path::MAIN_SEPARATOR;
        path.replace('.', MAIN_SEPARATOR.to_string().as_str())
    }

    fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(Self {
            remote: row.get(0)?,
            owner: row.get(1)?,
            name: row.get(2)?,
            path: row.get(3)?,
            pin: row.get(4)?,
            sync: row.get(5)?,
            last_visited_at: row.get(6)?,
            visited_count: row.get(7)?,
            new_created: false,
        })
    }
}

impl DisplayLevel {
    pub fn titles(&self) -> Vec<&'static str> {
        match self {
            Self::Remote => vec!["Remote", "Owner", "Name"],
            Self::Owner => vec!["Owner", "Name"],
            Self::Name => vec!["Name"],
        }
    }
}

impl ListItem for Repository {
    fn row<'a>(&'a self, title: &str) -> Cow<'a, str> {
        match title {
            "Remote" => Cow::Borrowed(&self.remote),
            "Owner" => Cow::Borrowed(&self.owner),
            "Name" => Cow::Borrowed(&self.name),
            "Flags" => {
                let mut flags = vec![];
                if self.sync {
                    flags.push("sync");
                }
                if self.pin {
                    flags.push("pin");
                }
                flags.join(",").into()
            }
            "LastVisited" => format_time(self.last_visited_at).into(),
            "Visited" => self.visited_count.to_string().into(),
            _ => Cow::Borrowed(""),
        }
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

    pub fn query(&self, opts: QueryOptions) -> Result<Vec<Repository>> {
        query(self.tx, opts)
    }

    pub fn count(&self, opts: QueryOptions) -> Result<u32> {
        count(self.tx, opts)
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
    pin INTEGER NOT NULL,
    sync INTEGER NOT NULL,
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
    pin,
    sync,
    last_visited_at,
    visited_count
) VALUES (
    ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8
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
            repo.pin,
            repo.sync,
            repo.last_visited_at,
            repo.visited_count,
        ],
    )?;
    Ok(())
}

const GET_SQL: &str = r#"
SELECT remote, owner, name, path, pin, sync, last_visited_at, visited_count
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
SELECT remote, owner, name, path, pin, sync, last_visited_at, visited_count
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
UPDATE repo SET last_visited_at = ?4, visited_count = ?5, pin = ?6, sync = ?7
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
            repo.pin,
            repo.sync,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct QueryOptions<'a> {
    pub remote: Option<&'a str>,
    pub owner: Option<&'a str>,
    pub name: Option<&'a str>,

    pub sync: Option<bool>,
    pub pin: Option<bool>,

    pub fuzzy: bool,

    pub limit: Option<LimitOptions>,
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

impl<'a> QueryOptions<'a> {
    fn build(&self, sql: &str) -> (String, Vec<Value>) {
        let mut conds = vec![];
        let mut limit = "";
        let mut idx = 0;
        let mut params = Vec::new();

        if let Some(limit_opts) = self.limit {
            idx += 2;
            limit = "LIMIT ?2 OFFSET ?1";
            params.push(Value::Integer(limit_opts.offset as i64));
            params.push(Value::Integer(limit_opts.limit as i64));
        }

        if let Some(remote) = self.remote {
            idx += 1;
            conds.push(format!("remote = ?{idx}"));
            params.push(Value::Text(remote.to_string()));
        }

        if let Some(owner) = self.owner {
            idx += 1;
            conds.push(format!("owner = ?{idx}"));
            params.push(Value::Text(owner.to_string()));
        }

        if let Some(name) = self.name {
            idx += 1;
            if self.fuzzy {
                conds.push(format!("name LIKE ?{idx}"));
                params.push(Value::Text(format!("%{name}%")));
            } else {
                conds.push(format!("name = ?{idx}"));
                params.push(Value::Text(name.to_string()));
            }
        }

        if let Some(sync) = self.sync {
            idx += 1;
            conds.push(format!("sync = ?{idx}"));
            params.push(Value::Integer(if sync { 1 } else { 0 }));
        }

        if let Some(pin) = self.pin {
            idx += 1;
            conds.push(format!("pin = ?{idx}"));
            params.push(Value::Integer(if pin { 1 } else { 0 }));
        }

        let wh = if conds.is_empty() {
            Cow::Borrowed("")
        } else {
            Cow::Owned(format!("WHERE {}", conds.join(" AND ")))
        };

        let sql = sql
            .replace("{{where}}", wh.as_ref())
            .replace("{{limit}}", limit)
            .replace('\n', " ");

        (sql, params)
    }
}

const QUERY_SQL: &str = r#"
SELECT remote, owner, name, path, pin, sync, last_visited_at, visited_count
FROM repo
{{where}}
ORDER BY last_visited_at DESC
{{limit}}
"#;

fn query(tx: &Transaction, opts: QueryOptions) -> Result<Vec<Repository>> {
    debug!("[db] Query repos, options: {opts:?}");
    let (sql, params) = opts.build(QUERY_SQL);
    debug!("[db] Query SQL: {sql:?}, params: {params:?}");
    let mut stmt = tx.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(params), Repository::from_row)?;
    let mut results = Vec::new();
    for row in rows {
        results.push(row?);
    }

    debug!("[db] Query results: {results:?}");
    Ok(results)
}

const COUNT_SQL: &str = r#"
SELECT COUNT(*) FROM repo {{where}}
"#;

fn count(tx: &Transaction, opts: QueryOptions) -> Result<u32> {
    debug!("[db] Count repos, options: {opts:?}");
    let (sql, params) = opts.build(COUNT_SQL);
    debug!("[db] Count SQL: {sql}, params: {params:?}");
    let count: u32 = tx.query_row(&sql, params_from_iter(params), |row| row.get(0))?;

    debug!("[db] Count result: {count}");
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
pub mod tests {
    use rusqlite::Connection;

    use super::*;

    #[test]
    fn test_get_path() {
        struct Case {
            remote: &'static str,
            owner: &'static str,
            name: &'static str,
            path: Option<&'static str>,
            expect: &'static str,
        }

        let cases = [
            Case {
                remote: "github",
                owner: "fioncat",
                name: "roxide",
                path: None,
                expect: "/dev/github/fioncat/roxide",
            },
            Case {
                remote: "github",
                owner: "fioncat",
                name: "roxide",
                path: Some("/myproject"),
                expect: "/myproject",
            },
            Case {
                remote: "gitlab",
                owner: "some/one",
                name: "otree",
                path: None,
                expect: "/dev/gitlab/some.one/otree",
            },
        ];

        for case in cases {
            let repo = Repository {
                remote: case.remote.to_string(),
                owner: case.owner.to_string(),
                name: case.name.to_string(),
                path: case.path.map(String::from),
                ..Default::default()
            };
            let path = repo.get_path("/dev");
            assert_eq!(path, PathBuf::from(case.expect));
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

    pub fn test_repos() -> Vec<Repository> {
        vec![
            Repository {
                remote: "github".to_string(),
                owner: "fioncat".to_string(),
                name: "roxide".to_string(),
                path: None,
                sync: true,
                pin: true,
                last_visited_at: 2234,
                visited_count: 20,
                new_created: false,
            },
            Repository {
                remote: "github".to_string(),
                owner: "fioncat".to_string(),
                name: "otree".to_string(),
                path: None,
                sync: true,
                pin: true,
                last_visited_at: 0,
                visited_count: 0,
                new_created: false,
            },
            Repository {
                remote: "github".to_string(),
                owner: "fioncat".to_string(),
                name: "nvimdots".to_string(),
                path: Some("/path/to/nvimdots".to_string()),
                sync: true,
                pin: true,
                last_visited_at: 3333,
                visited_count: 0,
                new_created: false,
            },
            Repository {
                remote: "github".to_string(),
                owner: "kubernetes".to_string(),
                name: "kubernetes".to_string(),
                path: None,
                sync: false,
                pin: true,
                last_visited_at: 7777,
                visited_count: 100,
                new_created: false,
            },
            Repository {
                remote: "gitlab".to_string(),
                owner: "fioncat".to_string(),
                name: "someproject".to_string(),
                path: None,
                sync: false,
                pin: false,
                last_visited_at: 1234,
                visited_count: 10,
                new_created: false,
            },
            Repository {
                remote: "gitlab".to_string(),
                owner: "fioncat".to_string(),
                name: "template".to_string(),
                path: None,
                sync: false,
                pin: false,
                last_visited_at: 2222,
                visited_count: 20,
                new_created: false,
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
    fn test_query_all() {
        let mut conn = build_conn();
        let tx = conn.transaction().unwrap();
        let repos = query(&tx, QueryOptions::default()).unwrap();
        let mut expects = test_repos();
        expects.sort_by(|a, b| b.last_visited_at.cmp(&a.last_visited_at));
        assert_eq!(repos, expects);

        let repos_limit = query(
            &tx,
            QueryOptions {
                limit: Some(LimitOptions {
                    offset: 1,
                    limit: 2,
                }),
                ..Default::default()
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
        let count = count(&tx, QueryOptions::default()).unwrap();
        let repos = test_repos();
        assert_eq!(count, repos.len() as u32);
    }

    #[test]
    fn test_query_by_remote() {
        let mut conn = build_conn();
        let tx = conn.transaction().unwrap();
        let repos = query(
            &tx,
            QueryOptions {
                remote: Some("github"),
                ..Default::default()
            },
        )
        .unwrap();
        let mut expects: Vec<Repository> = test_repos()
            .into_iter()
            .filter(|r| r.remote == "github")
            .collect();
        expects.sort_by(|a, b| b.last_visited_at.cmp(&a.last_visited_at));
        assert_eq!(repos, expects);
        let repos_limit = query(
            &tx,
            QueryOptions {
                remote: Some("github"),
                limit: Some(LimitOptions {
                    offset: 1,
                    limit: 1,
                }),
                ..Default::default()
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
        let result = count(
            &tx,
            QueryOptions {
                remote: Some("github"),
                ..Default::default()
            },
        )
        .unwrap();
        let repos = test_repos();
        let expect = repos.iter().filter(|r| r.remote == "github").count() as u32;
        assert_eq!(result, expect);

        let result = count(
            &tx,
            QueryOptions {
                remote: Some("gitlab"),
                ..Default::default()
            },
        )
        .unwrap();
        let expect = repos.iter().filter(|r| r.remote == "gitlab").count() as u32;
        assert_eq!(result, expect);
    }

    #[test]
    fn test_query_by_owner() {
        let mut conn = build_conn();
        let tx = conn.transaction().unwrap();
        let repos = query(
            &tx,
            QueryOptions {
                remote: Some("github"),
                owner: Some("fioncat"),
                ..Default::default()
            },
        )
        .unwrap();
        let mut expects: Vec<Repository> = test_repos()
            .into_iter()
            .filter(|r| r.remote == "github" && r.owner == "fioncat")
            .collect();
        expects.sort_by(|a, b| b.last_visited_at.cmp(&a.last_visited_at));
        assert_eq!(repos, expects);
        let repos_limit = query(
            &tx,
            QueryOptions {
                remote: Some("github"),
                owner: Some("fioncat"),
                limit: Some(LimitOptions {
                    offset: 1,
                    limit: 1,
                }),
                ..Default::default()
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
        let result = count(
            &tx,
            QueryOptions {
                remote: Some("github"),
                owner: Some("fioncat"),
                ..Default::default()
            },
        )
        .unwrap();
        let repos = test_repos();
        let expect = repos
            .iter()
            .filter(|r| r.remote == "github" && r.owner == "fioncat")
            .count() as u32;
        assert_eq!(result, expect);

        let result = count(
            &tx,
            QueryOptions {
                remote: Some("gitlab"),
                owner: Some("fioncat"),
                ..Default::default()
            },
        )
        .unwrap();
        let expect = repos
            .iter()
            .filter(|r| r.remote == "gitlab" && r.owner == "fioncat")
            .count() as u32;
        assert_eq!(result, expect);
    }

    #[test]
    fn test_query_by_pin() {
        let mut conn = build_conn();
        let tx = conn.transaction().unwrap();
        let repos = query(
            &tx,
            QueryOptions {
                pin: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        let mut expects: Vec<Repository> = test_repos().into_iter().filter(|r| r.pin).collect();
        expects.sort_by(|a, b| b.last_visited_at.cmp(&a.last_visited_at));
        assert_eq!(repos, expects);
    }

    #[test]
    fn test_count_by_pin() {
        let mut conn = build_conn();
        let tx = conn.transaction().unwrap();
        let result = count(
            &tx,
            QueryOptions {
                pin: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        let repos = test_repos();
        let expect = repos.iter().filter(|r| r.pin).count() as u32;
        assert_eq!(result, expect);
    }

    #[test]
    fn test_query_by_sync() {
        let mut conn = build_conn();
        let tx = conn.transaction().unwrap();
        let repos = query(
            &tx,
            QueryOptions {
                sync: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        let mut expects: Vec<Repository> = test_repos().into_iter().filter(|r| r.sync).collect();
        expects.sort_by(|a, b| b.last_visited_at.cmp(&a.last_visited_at));
        assert_eq!(repos, expects);
    }

    #[test]
    fn test_count_by_sync() {
        let mut conn = build_conn();
        let tx = conn.transaction().unwrap();
        let result = count(
            &tx,
            QueryOptions {
                sync: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        let repos = test_repos();
        let expect = repos.iter().filter(|r| r.sync).count() as u32;
        assert_eq!(result, expect);
    }

    #[test]
    fn test_query_fuzzy() {
        let mut conn = build_conn();
        let tx = conn.transaction().unwrap();
        let repos = query(
            &tx,
            QueryOptions {
                name: Some("o"),
                fuzzy: true,
                ..Default::default()
            },
        )
        .unwrap();
        let names = repos.iter().map(|r| r.name.as_str()).collect::<Vec<&str>>();
        assert_eq!(names, vec!["nvimdots", "roxide", "someproject", "otree"]);

        let repos = query(
            &tx,
            QueryOptions {
                remote: Some("github"),
                name: Some("s"),
                fuzzy: true,
                ..Default::default()
            },
        )
        .unwrap();
        let names = repos.iter().map(|r| r.name.as_str()).collect::<Vec<&str>>();
        assert_eq!(names, vec!["kubernetes", "nvimdots"]);

        let repos = query(
            &tx,
            QueryOptions {
                remote: Some("github"),
                owner: Some("fioncat"),
                name: Some("nvim"),
                fuzzy: true,
                ..Default::default()
            },
        )
        .unwrap();
        let names = repos.iter().map(|r| r.name.as_str()).collect::<Vec<&str>>();
        assert_eq!(names, vec!["nvimdots"]);
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

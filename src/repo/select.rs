use std::collections::HashSet;
use std::hash::Hash;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use reqwest::Url;

use crate::config::context::ConfigContext;
use crate::db::LimitOptions;
use crate::db::repo::{Repository, SearchLevel};
use crate::debug;
use crate::exec::fzf;

pub struct RepoSelector<'a> {
    ctx: Arc<ConfigContext>,

    head: &'a str,
    owner: &'a str,
    name: &'a str,

    limit: LimitOptions,

    fzf_filter: Option<&'static str>,
}

#[derive(Debug)]
enum SelectOneType<'a> {
    Url(Url),
    Ssh(&'a str),
    Direct,
}

impl<'a> RepoSelector<'a> {
    pub fn new(ctx: Arc<ConfigContext>, args: &'a [String]) -> Self {
        Self {
            ctx,
            head: args.first().map(|s| s.as_str()).unwrap_or_default(),
            owner: args.get(1).map(|s| s.as_str()).unwrap_or_default(),
            name: args.get(2).map(|s| s.as_str()).unwrap_or_default(),
            limit: LimitOptions::default(),
            fzf_filter: None,
        }
    }

    pub async fn select_one(&self, force_no_cache: bool, local: bool) -> Result<Repository> {
        debug!(
            "[select] Select one, head: {:?}, owner: {:?}, name: {:?}, local: {local}, force_no_cache: {force_no_cache}",
            self.head, self.owner, self.name,
        );
        if self.head.is_empty() {
            return self.select_one_from_db("", "", false);
        }

        if self.owner.is_empty() {
            let repo = self.select_one_from_head()?;
            if local && repo.new_created {
                bail!("cannot find repo from url or ssh");
            }
            return Ok(repo);
        }

        if self.name.is_empty() {
            return self.select_one_from_owner(force_no_cache, local).await;
        }

        self.select_one_from_name(local)
    }

    fn select_one_from_head(&self) -> Result<Repository> {
        debug!("[select] Select one from head");
        if self.head == "-" {
            debug!("[select] Select latest from db");
            // Select latest repo from db
            return self.select_one_from_db("", "", true);
        }

        // If only one `head` parameter is provided, its meaning needs to be
        // inferred based on its format:
        // - It could be a URL.
        // - It could be a clone SSH.
        // - It could be a remote name.
        // - It could be a fuzzy keyword.
        let select_type = if self.head.ends_with(".git") {
            // If `head` ends with `".git"`, by default, consider it as a clone
            // URL, which could be in either HTTP or SSH format. However, it
            // could also be just a remote or keyword ending with `".git"`.
            if self.head.starts_with("http") {
                let url = self.head.trim_end_matches(".git");
                SelectOneType::Url(
                    Url::parse(url).with_context(|| format!("parse clone url '{url}'"))?,
                )
            } else if self.head.starts_with("git@") {
                SelectOneType::Ssh(self.head)
            } else {
                // If it is neither HTTP nor SSH, consider it not to be a
                // clone URL.
                match Url::parse(self.head) {
                    Ok(url) => SelectOneType::Url(url),
                    Err(_) => SelectOneType::Direct,
                }
            }
        } else {
            // We prioritize treating `head` as a URL, so we attempt to parse
            // it using URL rules. If parsing fails, then consider it as a
            // remote or keyword.
            match Url::parse(self.head) {
                Ok(url) => SelectOneType::Url(url),
                Err(_) => SelectOneType::Direct,
            }
        };
        debug!("[select] Select type: {select_type:?}");

        match select_type {
            SelectOneType::Url(url) => self.select_one_from_url(&url),
            SelectOneType::Ssh(ssh) => self.select_one_from_ssh(ssh),
            SelectOneType::Direct => {
                // Treating `head` as a remote (with higher priority) or fuzzy matching
                // keyword, we will call different functions from the database to retrieve
                // the information.
                if self.ctx.cfg.contains_remote(self.head) {
                    debug!(
                        "[select] Head {:?} is a remote, select one from db",
                        self.head
                    );
                    return self.select_one_from_db(self.head, "", false);
                }

                debug!(
                    "[select] Head {:?} is a keyword, fuzzy select from db",
                    self.head
                );
                self.select_one_fuzzy("", "", self.head)
            }
        }
    }

    const GITHUB_DOMAIN: &'static str = "github.com";

    fn select_one_from_url(&self, url: &Url) -> Result<Repository> {
        debug!("[select] Select one from url: {url:?}");

        let Some(domain) = url.domain() else {
            bail!("cannot get domain from url: {url:?}");
        };

        let mut target_remote = None;
        let mut is_gitlab = false;

        for remote in self.ctx.cfg.remotes.iter() {
            // We match the domain of the URL based on the clone domain of the
            // remote. This is because in many cases, the provided URL is a clone
            // address, and even for access addresses, most of the time their
            // domains are consistent with the clone.
            // TODO: Provide another match domain in config?
            let Some(ref remote_domain) = remote.clone else {
                continue;
            };
            if remote_domain != domain {
                continue;
            }

            // We only support parsing two types of URLs: GitHub and GitLab. For
            // non-GitHub cases, we consider them all as GitLab.
            // TODO: Add support for parsing URLs from more types of remotes.
            if remote_domain != Self::GITHUB_DOMAIN {
                is_gitlab = true;
            }
            target_remote = Some(remote);
            break;
        }

        let Some(remote) = target_remote else {
            bail!("no matching remote found for domain: {domain:?}");
        };

        let Some(path_iter) = url.path_segments() else {
            bail!("invalid url {url:?}, path cannot be empty");
        };
        debug!(
            "[select] Target remote: {}, is_gitlab: {is_gitlab}",
            remote.name
        );

        // We use a simple method to parse repository URL:
        //
        // - For GitHub, both owner and name are required, and sub-owners are not
        // supported. Therefore, as long as two path segments are identified, it
        // is considered within a repository. The subsequent path is assumed to be
        // the branch or file path.
        //
        // - For GitLab, the presence of sub-owners complicates direct localization
        // of two segments. The path rule in GitLab is that starting from "-", the
        // subsequent path is the branch or file. Therefore, locating the "-" is
        // sufficient for GitLab.
        let mut segs = Vec::new(); // The segments for repository, contains owner and name
        for part in path_iter {
            if is_gitlab {
                if part == "-" {
                    break;
                }
                segs.push(part);
                continue;
            }

            if segs.len() == 2 {
                break;
            }
            segs.push(part);
        }

        debug!("[select] Parsed url segments: {segs:?}");
        // The owner and name are both required for GitHub and GitLab, so the length
        // of `segs` should be bigger than 2.
        // If not, it means that user are not in a repository, maybe in an owner.
        if segs.len() < 2 {
            bail!("invalid url {url:?}, should be in a repo");
        }

        let path = segs.join("/");
        let (owner, name) = split_owner(path);

        debug!("[select] Parsed owner: {owner:?}, name: {name:?}");
        let repo = self.get_or_create(&remote.name, &owner, &name)?;
        debug!("[select] Select repo: {repo:?}");
        Ok(repo)
    }

    fn select_one_from_ssh(&self, ssh: &str) -> Result<Repository> {
        // Parsing SSH is done in a clever way by reusing the code for parsing
        // URLs. The approach involves converting the SSH statement to a URL and
        // then calling the URL parsing code.
        let full_name = ssh
            .strip_prefix("git@")
            .unwrap()
            .strip_suffix(".git")
            .unwrap();
        let full_name = full_name.replacen(':', "/", 1);

        let convert_url = format!("https://{full_name}");
        let url = Url::parse(&convert_url).with_context(|| {
            format!("invalid ssh, parse url {convert_url:?} converted from ssh {ssh:?}")
        })?;

        self.select_one_from_url(&url)
    }

    async fn select_one_from_owner(
        &self,
        force_no_cache: bool,
        mut local: bool,
    ) -> Result<Repository> {
        debug!("[select] Select one from owner");
        let remote = self.ctx.cfg.get_remote(self.head)?;
        if remote.api.is_none() && !local {
            debug!(
                "[select] Remote {:?} does not have api, force to use local mode",
                remote.name
            );
            local = true;
        }
        if self.owner == "-" {
            debug!("[select] Select latest from db");
            return self.select_one_from_db(self.head, "", true);
        }

        let db = self.ctx.get_db()?;
        if local {
            debug!("[select] Local mode, select from db");
            let count =
                db.with_transaction(|tx| tx.repo().count_by_owner(&remote.name, self.owner))?;
            if count > 0 {
                debug!("[select] {:?} is an owner, select from db", self.owner);
                return self.select_one_from_db(self.head, self.owner, false);
            }

            // This is not an owner, consider it as a fuzzy keyword to search
            debug!(
                "[select] {:?} is not an owner, fuzzy select from db",
                self.owner
            );
            let name = self.owner;
            return self.select_one_fuzzy(&remote.name, "", name);
        }

        debug!("[select] No local mode, list remote repos");
        let api = self.ctx.get_api(remote.name.as_str(), force_no_cache)?;
        let remote_repos = api.list_repos(&remote.name, self.owner).await?;
        if remote_repos.is_empty() {
            bail!("no new repo found in {:?}", self.owner);
        }

        let locals = db
            .with_transaction(|tx| {
                tx.repo()
                    .query_by_owner(&remote.name, self.owner, self.limit)
            })?
            .into_iter()
            .map(|r| r.name)
            .collect::<Vec<_>>();
        debug!("[select] Local repos: {locals:?}, remote repos: {remote_repos:?}");
        let items = merge_sorted(locals, remote_repos);

        debug!("[select] Use fzf to select items: {items:?}");
        let idx = fzf::search("Search remote repos", &items, self.fzf_filter)?;
        let name = &items[idx];
        let repo = self.get_or_create(&remote.name, self.owner, name)?;
        debug!("[select] Select repo: {repo:?}");
        Ok(repo)
    }

    fn select_one_from_name(&self, local: bool) -> Result<Repository> {
        debug!("[select] Select one from name");
        if self.name == "-" {
            debug!("[select] Select latest from db");
            return self.select_one_from_db(self.head, self.owner, true);
        }

        let db = self.ctx.get_db()?;
        let remote = self.ctx.cfg.get_remote(self.head)?;
        let count = db.with_transaction(|tx| tx.repo().count_by_owner(&remote.name, self.owner))?;
        if count == 0 {
            bail!("no repo found in owner {:?}", self.owner);
        }

        if local {
            debug!("[select] Local mode, fuzzy select from db");
            return self.select_one_fuzzy(&remote.name, self.owner, self.name);
        }
        debug!("[select] No local mode, get or create from db");
        self.get_or_create(&remote.name, self.owner, self.name)
    }

    fn select_one_fuzzy(&self, remote: &str, owner: &str, name: &str) -> Result<Repository> {
        debug!("[select] Select one fuzzy, remote: {remote:?}, owner: {owner:?}, name: {name:?}");
        let db = self.ctx.get_db()?;

        let repos =
            db.with_transaction(|tx| tx.repo().query_fuzzy(remote, owner, name, self.limit))?;

        for repo in repos {
            let path = repo.get_path(&self.ctx.cfg.workspace);
            if self.ctx.current_dir.starts_with(&path) {
                debug!("[select] The current dir is under repo {repo:?}, skip it");
                continue;
            }

            debug!("[select] Fuzzy select repo: {repo:?}");
            return Ok(repo);
        }

        bail!("no repo found by fuzzy query");
    }

    fn select_one_from_db(&self, remote: &str, owner: &str, latest: bool) -> Result<Repository> {
        debug!(
            "[select] Select one from db, remote: {remote:?}, owner: {owner:?}, latest: {latest}"
        );
        let db = self.ctx.get_db()?;
        let mut level = SearchLevel::Name;
        let repos = db.with_transaction(|tx| {
            if remote.is_empty() {
                level = SearchLevel::Remote;
                tx.repo().query_all(self.limit)
            } else if owner.is_empty() {
                level = SearchLevel::Owner;
                tx.repo().query_by_remote(remote, self.limit)
            } else {
                tx.repo().query_by_owner(remote, owner, self.limit)
            }
        })?;

        let mut filtered = Vec::with_capacity(repos.len());
        for repo in repos {
            let path = repo.get_path(&self.ctx.cfg.workspace);
            if self.ctx.current_dir == path {
                debug!("[select] The current dir equals to repo {repo:?}, skip it");
                continue;
            }

            if remote.is_empty() && self.ctx.current_dir.starts_with(&path) {
                debug!("[select] The current dir is under repo {repo:?}, select it directly");
                return Ok(repo);
            }
            filtered.push(repo);
        }
        let mut repos = filtered;

        if repos.is_empty() {
            bail!("no repo to select");
        }

        if latest {
            debug!("[select] Select latest repo {:?}", repos[0]);
            return Ok(repos.remove(0));
        }

        let items = repos
            .iter()
            .map(|r| r.search_item(level))
            .collect::<Vec<_>>();
        debug!("[select] Use fzf to select items: {items:?}");
        let idx = fzf::search("Select one repo from database", &items, self.fzf_filter)?;
        debug!("[select] Select repo: {:?}", repos[idx]);
        Ok(repos.remove(idx))
    }

    fn get_or_create(&self, remote: &str, owner: &str, name: &str) -> Result<Repository> {
        let db = self.ctx.get_db()?;
        let repo = db.with_transaction(|tx| tx.repo().get_optional(remote, owner, name))?;
        Ok(match repo {
            Some(repo) => repo,
            None => self.new_repo(remote, owner, name),
        })
    }

    fn new_repo(&self, remote: &str, owner: &str, name: &str) -> Repository {
        Repository {
            remote: remote.to_string(),
            owner: owner.to_string(),
            name: name.to_string(),
            ..Default::default()
        }
    }
}

/// Parsing a path into owner and name follows the basic format: `"{owner}/{name}"`.
/// `"{owner}"` adheres to GitLab's rules and can include sub-owners (i.e., multiple
/// levels of directories). If the path does not contain `"/"`, then return the
/// path directly with an empty owner.
pub fn split_owner(path: impl AsRef<str>) -> (String, String) {
    let items: Vec<_> = path.as_ref().split('/').collect();
    let items_len = items.len();
    let mut group_buffer: Vec<String> = Vec::with_capacity(items_len - 1);
    let mut base = "";
    for (idx, item) in items.iter().enumerate() {
        if idx == items_len - 1 {
            base = item;
        } else {
            group_buffer.push(item.to_string());
        }
    }
    (group_buffer.join("/"), base.to_string())
}

fn merge_sorted<T>(sorted: Vec<T>, unsorted: Vec<T>) -> Vec<T>
where
    T: Clone + Eq + Hash,
{
    let sorted_set: HashSet<T> = sorted.iter().cloned().collect();

    let intersection: Vec<T> = sorted
        .into_iter()
        .filter(|x| unsorted.contains(x))
        .collect();

    let remaining: Vec<T> = unsorted
        .into_iter()
        .filter(|x| !sorted_set.contains(x))
        .collect();

    let mut result = Vec::with_capacity(intersection.len() + remaining.len());
    result.extend(intersection);
    result.extend(remaining);

    result
}

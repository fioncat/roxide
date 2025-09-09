use std::collections::HashSet;
use std::hash::Hash;

use anyhow::{Context, Result, bail};
use clap::Args;
use reqwest::Url;

use crate::api::{
    HeadRepository, ListPullRequestsOptions, PullRequest, PullRequestHead, RemoteAPI,
};
use crate::config::context::ConfigContext;
use crate::db::repo::{
    DisplayLevel, LimitOptions, OwnerState, QueryOptions, RemoteState, Repository,
};
use crate::debug;
use crate::exec::git::branch::Branch;
use crate::repo::current::get_current_repo;
use crate::term::list::List;

#[derive(Debug, Args, Default)]
pub struct SelectRepoArgs {
    pub head: Option<String>,

    pub owner: Option<String>,

    pub name: Option<String>,
}

pub struct RepoSelector<'a, 'b> {
    ctx: &'a ConfigContext,

    head: &'b str,
    owner: &'b str,
    name: &'b str,

    fzf_filter: Option<&'static str>,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SelectManyReposOptions {
    pub sync: Option<bool>,
    pub pin: Option<bool>,
    pub limit: Option<LimitOptions>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoList {
    pub items: Vec<Repository>,
    pub total: u32,
    pub level: DisplayLevel,
}

#[derive(Debug)]
enum SelectOneType<'a> {
    Url(Url),
    Ssh(&'a str),
    Direct,
}

impl<'a, 'b> RepoSelector<'a, 'b> {
    pub fn new(ctx: &'a ConfigContext, args: &'b SelectRepoArgs) -> Self {
        debug!("[select] Create repo selector, args: {args:?}");
        Self {
            ctx,
            head: args.head.as_deref().unwrap_or(""),
            owner: args.owner.as_deref().unwrap_or(""),
            name: args.name.as_deref().unwrap_or(""),
            fzf_filter: None,
        }
    }

    #[cfg(test)]
    pub fn from_args(ctx: &'a ConfigContext, args: &'b [String]) -> Self {
        Self {
            ctx,
            head: args.first().map(|s| s.as_str()).unwrap_or(""),
            owner: args.get(1).map(|s| s.as_str()).unwrap_or(""),
            name: args.get(2).map(|s| s.as_str()).unwrap_or(""),
            fzf_filter: None,
        }
    }

    pub async fn select_one(&self, force_no_cache: bool, local: bool) -> Result<Repository> {
        debug!("[select] Select one, local: {local}, force_no_cache: {force_no_cache}");
        if self.head.is_empty() {
            return self.select_one_from_db("", "", false);
        }

        if self.owner.is_empty() {
            let repo = self.select_one_from_head(false)?;
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

    pub async fn select_remote(&self, force_no_cache: bool) -> Result<Repository> {
        let repo = self.select_remote_inner(force_no_cache).await?;
        if !repo.new_created {
            bail!("repo {:?} already exists locally", repo.full_name());
        }
        Ok(repo)
    }

    async fn select_remote_inner(&self, force_no_cache: bool) -> Result<Repository> {
        debug!("[select] Select one remote repo, force_no_cache: {force_no_cache}");
        if self.head.is_empty() {
            bail!("head cannot be empty when selecting from remote");
        }

        if self.owner.is_empty() {
            return self.select_one_from_head(true);
        }

        if !self.name.is_empty() {
            debug!("[select] Select one remote repo from name");
            return self.get_or_create(self.head, self.owner, self.name);
        }

        let remote = self.ctx.cfg.get_remote(self.head)?;
        let api = self.ctx.get_api(&remote.name, force_no_cache)?;
        let remote_repos = api.list_repos(&remote.name, self.owner).await?;
        if remote_repos.is_empty() {
            bail!("no remote repo in {:?}", self.owner);
        }

        let db = self.ctx.get_db()?;
        let locals = db
            .with_transaction(|tx| {
                tx.repo().query(QueryOptions {
                    remote: Some(&remote.name),
                    owner: Some(self.owner),
                    ..Default::default()
                })
            })?
            .into_iter()
            .map(|r| r.name)
            .collect::<HashSet<_>>();

        let remote_repos = remote_repos
            .into_iter()
            .filter(|r| !locals.contains(r))
            .collect::<Vec<_>>();
        if remote_repos.is_empty() {
            bail!(
                "all repos in {:?} exist locally, no new repo to select",
                self.owner
            );
        }

        let idx = self
            .ctx
            .fzf_search("Search remote repos", &remote_repos, self.fzf_filter)?;
        let name = &remote_repos[idx];
        let repo = self.get_or_create(&remote.name, self.owner, name)?;
        debug!("[select] Select remote repo: {repo:?}");
        Ok(repo)
    }

    fn select_one_from_head(&self, remote_mode: bool) -> Result<Repository> {
        debug!("[select] Select one from head, remote_mode: {remote_mode}");
        if self.head == "-" {
            debug!("[select] Select latest from db");
            if remote_mode {
                bail!("cannot use '-' to select latest when selecting from remote");
            }
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
                if remote_mode {
                    bail!("the head must be url or ssh when selecting from remote");
                }
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
            let count = db.with_transaction(|tx| {
                tx.repo().count(QueryOptions {
                    remote: Some(&remote.name),
                    owner: Some(self.owner),
                    ..Default::default()
                })
            })?;
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
                tx.repo().query(QueryOptions {
                    remote: Some(&remote.name),
                    owner: Some(self.owner),
                    ..Default::default()
                })
            })?
            .into_iter()
            .map(|r| r.name)
            .collect::<Vec<_>>();
        debug!("[select] Local repos: {locals:?}, remote repos: {remote_repos:?}");
        let items = merge_sorted(locals, remote_repos);

        debug!("[select] Use fzf to select items: {items:?}");
        let idx = self
            .ctx
            .fzf_search("Search repos", &items, self.fzf_filter)?;
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

        let remote = self.ctx.cfg.get_remote(self.head)?;

        if local {
            let db = self.ctx.get_db()?;
            let count = db.with_transaction(|tx| {
                tx.repo().count(QueryOptions {
                    remote: Some(&remote.name),
                    owner: Some(self.owner),
                    ..Default::default()
                })
            })?;
            if count == 0 {
                bail!("no repo found in owner {:?}", self.owner);
            }
            debug!("[select] Local mode, fuzzy select from db");
            return self.select_one_fuzzy(&remote.name, self.owner, self.name);
        }

        debug!("[select] No local mode, get or create from db");
        self.get_or_create(&remote.name, self.owner, self.name)
    }

    fn select_one_fuzzy(&self, remote: &str, owner: &str, name: &str) -> Result<Repository> {
        debug!("[select] Select one fuzzy, remote: {remote:?}, owner: {owner:?}, name: {name:?}");
        let db = self.ctx.get_db()?;

        let repos = db.with_transaction(|tx| {
            tx.repo().query(QueryOptions {
                remote: if remote.is_empty() {
                    None
                } else {
                    Some(remote)
                },
                owner: if owner.is_empty() { None } else { Some(owner) },
                name: Some(name),
                fuzzy: true,
                ..Default::default()
            })
        })?;

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
        let mut level = DisplayLevel::Remote;
        let repos = db.with_transaction(|tx| {
            let mut opts = QueryOptions::default();
            if !remote.is_empty() {
                opts.remote = Some(remote);
                level = DisplayLevel::Owner;
            }

            if !owner.is_empty() {
                opts.owner = Some(owner);
                level = DisplayLevel::Name;
            }

            tx.repo().query(opts)
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
        let idx = self
            .ctx
            .fzf_search("Select one repo from database", &items, self.fzf_filter)?;
        debug!("[select] Select repo: {:?}", repos[idx]);
        Ok(repos.remove(idx))
    }

    fn get_or_create(&self, remote: &str, owner: &str, name: &str) -> Result<Repository> {
        let db = self.ctx.get_db()?;
        let repo = db.with_transaction(|tx| tx.repo().get(remote, owner, name))?;
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
            new_created: true,
            ..Default::default()
        }
    }

    pub fn select_many(&self, opts: SelectManyReposOptions) -> Result<RepoList> {
        debug!("[select] Select many, opts: {opts:?}");

        let mut query_opts = QueryOptions {
            sync: opts.sync,
            pin: opts.pin,
            limit: opts.limit,
            ..Default::default()
        };

        let mut level = DisplayLevel::Remote;
        if !self.head.is_empty() {
            query_opts.remote = Some(self.head);
            level = DisplayLevel::Owner;
        }

        if !self.owner.is_empty() {
            query_opts.owner = Some(self.owner);
            level = DisplayLevel::Name;
        }

        if !self.name.is_empty() {
            query_opts.name = Some(self.name);
        }

        debug!("[select] Query options: {query_opts:?}");
        let (repos, total) = self.ctx.get_db()?.with_transaction(|tx| {
            let repos = tx.repo().query(query_opts)?;
            let total = tx.repo().count(query_opts)?;
            Ok((repos, total))
        })?;

        let result = RepoList {
            items: repos,
            total,
            level,
        };
        debug!("[select] Select many result: {result:?}");
        Ok(result)
    }
}

impl RepoList {
    #[inline]
    pub fn display_names(&self) -> Vec<String> {
        self.items
            .iter()
            .map(|r| r.display_name(self.level))
            .collect()
    }
}

impl List<Repository> for RepoList {
    fn titles(&self) -> Vec<&'static str> {
        let mut titles = self.level.titles();
        titles.extend(vec!["Flags", "Visited", "LastVisited"]);
        titles
    }

    fn total(&self) -> u32 {
        self.total
    }

    fn items(&self) -> &[Repository] {
        self.items.as_ref()
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteList {
    pub remotes: Vec<RemoteState>,
    pub total: u32,
}

impl List<RemoteState> for RemoteList {
    fn titles(&self) -> Vec<&'static str> {
        vec!["Remote", "OwnerCount", "RepoCount"]
    }

    fn total(&self) -> u32 {
        self.total
    }

    fn items(&self) -> &[RemoteState] {
        &self.remotes
    }
}

pub fn select_remotes(ctx: &ConfigContext, limit: LimitOptions) -> Result<RemoteList> {
    debug!("[select] Select remotes, limit: {limit:?}");
    let db = ctx.get_db()?;
    let (remotes, total) = db.with_transaction(|tx| {
        let total = tx.repo().count_remotes()?;
        let remotes = tx.repo().query_remotes(Some(limit))?;
        Ok((remotes, total))
    })?;
    let list = RemoteList { remotes, total };
    debug!("[select] Result: {list:?}");
    Ok(list)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnerList {
    pub show_remote: bool,
    pub owners: Vec<OwnerState>,
    pub total: u32,
}

impl List<OwnerState> for OwnerList {
    fn titles(&self) -> Vec<&'static str> {
        if self.show_remote {
            vec!["Remote", "Owner", "RepoCount"]
        } else {
            vec!["Owner", "RepoCount"]
        }
    }

    fn total(&self) -> u32 {
        self.total
    }

    fn items(&self) -> &[OwnerState] {
        &self.owners
    }
}

pub fn select_owners(
    ctx: &ConfigContext,
    remote: Option<String>,
    limit: LimitOptions,
) -> Result<OwnerList> {
    debug!("[select] Select owners, remote: {remote:?}, limit: {limit:?}");
    let db = ctx.get_db()?;

    let show_remote = remote.is_none();
    let (owners, total) = db.with_transaction(|tx| {
        let total = tx.repo().count_owners(remote.as_deref())?;
        let owners = tx.repo().query_owners(remote, Some(limit))?;
        Ok((owners, total))
    })?;

    let list = OwnerList {
        show_remote,
        owners,
        total,
    };
    debug!("[select] Result: {list:?}");
    Ok(list)
}

#[derive(Debug, Args, Default)]
pub struct SelectPullRequestsArgs {
    pub id: Option<u64>,

    #[arg(long, short)]
    pub upstream: bool,

    #[arg(long, short)]
    pub all: bool,

    #[arg(long, short)]
    pub base: Option<String>,
}

impl SelectPullRequestsArgs {
    pub async fn select_one(
        self,
        ctx: &ConfigContext,
        force_no_cache: bool,
        filter: Option<&str>,
    ) -> Result<PullRequest> {
        debug!("[select] Select one pull request, args: {:?}", self);
        let mut prs = self.select_many(ctx, force_no_cache).await?;
        if prs.is_empty() {
            bail!("no pull request found");
        }

        if prs.len() == 1 {
            return Ok(prs.remove(0));
        }

        let items = PullRequest::search_items(&prs);
        debug!("[select] Multiple pull requests found, use fzf to select one");
        let idx = ctx.fzf_search("Select one pull request", &items, filter)?;
        Ok(prs.remove(idx))
    }

    pub async fn select_many(
        self,
        ctx: &ConfigContext,
        force_no_cache: bool,
    ) -> Result<Vec<PullRequest>> {
        debug!("[select] Select pull requests, args: {:?}", self);
        let repo = get_current_repo(ctx)?;
        let api = ctx.get_api(&repo.remote, force_no_cache)?;
        let opts = self
            .build_list_options(ctx, &repo, api.as_ref(), false)
            .await?;
        let prs = api.list_pull_requests(opts).await?;
        debug!("[select] Pull requests: {prs:?}");
        Ok(prs)
    }

    pub async fn build_list_options(
        self,
        ctx: &ConfigContext,
        repo: &Repository,
        api: &dyn RemoteAPI,
        must_base: bool,
    ) -> Result<ListPullRequestsOptions> {
        let (owner, name, base) = if self.upstream {
            let api_repo = api.get_repo(&repo.remote, &repo.owner, &repo.name).await?;
            let Some(upstream) = api_repo.upstream else {
                bail!("repo has no upstream");
            };
            if must_base && self.base.is_none() {
                (upstream.owner, upstream.name, Some(upstream.default_branch))
            } else {
                (upstream.owner, upstream.name, self.base)
            }
        } else {
            let owner = repo.owner.clone();
            let name = repo.name.clone();
            if must_base && self.base.is_none() {
                let default_branch = Branch::default(ctx.git().mute())?;
                (owner, name, Some(default_branch))
            } else {
                (owner, name, self.base)
            }
        };

        let head = if self.all {
            None
        } else {
            let current_branch = Branch::current(ctx.git().mute())?;
            if self.upstream {
                Some(PullRequestHead::Repository(HeadRepository {
                    owner: repo.owner.clone(),
                    name: repo.name.clone(),
                    branch: current_branch,
                }))
            } else {
                Some(PullRequestHead::Branch(current_branch))
            }
        };

        Ok(ListPullRequestsOptions {
            owner,
            name,
            id: self.id,
            head,
            base,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::env;

    use crate::config::context;

    use super::*;

    #[tokio::test]
    async fn test_select_one_empty() {
        let ctx = context::tests::build_test_context("select_one_empty");

        let mut selector = RepoSelector::from_args(&ctx, &[]);
        selector.fzf_filter = Some("roxide");
        let repo = selector.select_one(false, false).await.unwrap();
        assert_eq!(
            repo,
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
            }
        );
    }

    #[tokio::test]
    async fn test_select_one_empty_in_repo_root() {
        let current_dir = env::current_dir().unwrap();
        let repo_path = current_dir
            .join("tests")
            .join("select_one_empty_in_repo_root")
            .join("workspace")
            .join("github")
            .join("fioncat")
            .join("roxide");
        let mut ctx = context::tests::build_test_context("select_one_empty_in_repo_root");
        ctx.current_dir = repo_path;

        let mut selector = RepoSelector::from_args(&ctx, &[]);
        selector.fzf_filter = Some("roxide");
        // We are in roxide, it should be excluded
        let result = selector.select_one(false, false).await;
        assert_eq!(result.err().unwrap().to_string(), "fzf no match found");
    }

    #[tokio::test]
    async fn test_select_one_empty_in_repo_subdir() {
        let current_dir = env::current_dir().unwrap();
        let repo_path = current_dir
            .join("tests")
            .join("select_one_empty_in_repo_subdir")
            .join("workspace")
            .join("github")
            .join("fioncat")
            .join("roxide")
            .join("src")
            .join("subdir");
        let mut ctx = context::tests::build_test_context("select_one_empty_in_repo_subdir");
        ctx.current_dir = repo_path;

        let mut selector = RepoSelector::from_args(&ctx, &[]);
        // This should be ignored, because we are in a subdir of roxide
        // The selector should select it directly
        selector.fzf_filter = Some("non-exists");
        let repo = selector.select_one(false, false).await.unwrap();
        assert_eq!(
            repo,
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
            }
        );
    }

    #[tokio::test]
    async fn test_select_one_url_ssh() {
        let ctx = context::tests::build_test_context("select_one_url_ssh");

        #[derive(Default)]
        struct Case {
            url: &'static str,
            expect: &'static str,
            new_created: bool,
            should_ok: bool,
        }

        let cases = [
            Case {
                url: "https://github.com/fioncat/roxide",
                expect: "github:fioncat/roxide",
                new_created: false,
                should_ok: true,
            },
            Case {
                url: "https://github.com/fioncat/roxide.git",
                expect: "github:fioncat/roxide",
                new_created: false,
                should_ok: true,
            },
            Case {
                url: "https://github.com/fioncat/nvimdots/tree/custom/lua/modules",
                expect: "github:fioncat/nvimdots",
                new_created: false,
                should_ok: true,
            },
            Case {
                url: "https://github.com/fioncat/kubernetes",
                expect: "github:fioncat/kubernetes",
                new_created: true,
                should_ok: true,
            },
            Case {
                url: "https://github.com/fioncat",
                should_ok: false,
                ..Default::default()
            },
            Case {
                url: "https://github.com",
                should_ok: false,
                ..Default::default()
            },
            Case {
                url: "https://hello.com",
                should_ok: false,
                ..Default::default()
            },
            Case {
                url: "https://git.mydomain.com/fioncat/someproject/-/tree/master/deploy",
                expect: "gitlab:fioncat/someproject",
                new_created: false,
                should_ok: true,
            },
            Case {
                url: "https://git.mydomain.com/fioncat/template",
                expect: "gitlab:fioncat/template",
                new_created: false,
                should_ok: true,
            },
            Case {
                url: "https://git.mydomain.com/group/subgroup/someproject/-/tree/main/src",
                expect: "gitlab:group.subgroup/someproject",
                new_created: true,
                should_ok: true,
            },
            Case {
                url: "git@github.com:fioncat/nvimdots.git",
                expect: "github:fioncat/nvimdots",
                new_created: false,
                should_ok: true,
            },
            Case {
                url: "git@git.mydomain.com:fioncat/someproject.git",
                expect: "gitlab:fioncat/someproject",
                new_created: false,
                should_ok: true,
            },
        ];

        for case in cases {
            let url = case.url.to_string();
            let args = vec![url];
            let selector = RepoSelector::from_args(&ctx, &args);
            let result = selector.select_one(false, false).await;
            if !case.should_ok {
                assert!(result.is_err());
                continue;
            }
            let repo = result.unwrap();
            assert_eq!(repo.new_created, case.new_created);
            assert_eq!(repo.full_name(), case.expect);
        }
    }

    #[tokio::test]
    async fn test_select_one_remote() {
        let ctx = context::tests::build_test_context("select_one_remote");
        let args = vec!["github".to_string()];
        let mut selector = RepoSelector::from_args(&ctx, &args);
        selector.fzf_filter = Some("roxide");
        let repo = selector.select_one(false, false).await.unwrap();
        assert_eq!(
            repo,
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
            }
        );
    }

    #[tokio::test]
    async fn test_select_one_remote_latest() {
        let ctx = context::tests::build_test_context("select_one_remote_latest");
        let args = vec!["-".to_string()];
        let selector = RepoSelector::from_args(&ctx, &args);
        let repo = selector.select_one(false, false).await.unwrap();
        assert_eq!(
            repo,
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
            }
        );
    }

    #[tokio::test]
    async fn test_select_one_remote_fuzzy() {
        let ctx = context::tests::build_test_context("select_one_remote_fuzzy");
        let args = vec!["rox".to_string()];
        let selector = RepoSelector::from_args(&ctx, &args);
        let repo = selector.select_one(false, false).await.unwrap();
        assert_eq!(
            repo,
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
            }
        );
    }

    #[tokio::test]
    async fn test_select_one_owner() {
        let ctx = context::tests::build_test_context("select_one_owner");
        let args = vec!["github".to_string(), "fioncat".to_string()];
        let mut selector = RepoSelector::from_args(&ctx, &args);
        selector.fzf_filter = Some("dotfiles");
        let repo = selector.select_one(false, false).await.unwrap();
        assert_eq!(
            repo,
            Repository {
                remote: "github".to_string(),
                owner: "fioncat".to_string(),
                name: "dotfiles".to_string(),
                new_created: true,
                ..Default::default()
            }
        );
    }

    #[tokio::test]
    async fn test_select_one_owner_latest() {
        let ctx = context::tests::build_test_context("select_one_owner_latest");
        let args = vec!["github".to_string(), "-".to_string()];
        let selector = RepoSelector::from_args(&ctx, &args);
        let repo = selector.select_one(false, false).await.unwrap();
        assert_eq!(
            repo,
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
            }
        );
    }

    #[tokio::test]
    async fn test_select_one_owner_local() {
        let ctx = context::tests::build_test_context("select_one_owner_local");
        let args = vec!["github".to_string(), "fioncat".to_string()];
        let mut selector = RepoSelector::from_args(&ctx, &args);
        selector.fzf_filter = Some("roxide");
        let repo = selector.select_one(false, true).await.unwrap();
        assert_eq!(
            repo,
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
            }
        );

        // Now the remote repository won't be selected
        selector.fzf_filter = Some("dotfiles");
        let result = selector.select_one(false, true).await;
        assert_eq!(result.err().unwrap().to_string(), "fzf no match found");
    }

    #[tokio::test]
    async fn test_select_one_owner_local_fuzzy() {
        let ctx = context::tests::build_test_context("select_one_owner_local_fuzzy");
        // `rox` is not an owner, in local mode, it should be treated as a fuzzy keyword
        let args = vec!["github".to_string(), "rox".to_string()];
        let selector = RepoSelector::from_args(&ctx, &args);
        let repo = selector.select_one(false, true).await.unwrap();
        assert_eq!(
            repo,
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
            }
        );
    }

    #[tokio::test]
    async fn test_select_one_name() {
        let ctx = context::tests::build_test_context("select_one_name");
        let args = vec![
            "github".to_string(),
            "fioncat".to_string(),
            "roxide".to_string(),
        ];
        let selector = RepoSelector::from_args(&ctx, &args);
        let repo = selector.select_one(false, false).await.unwrap();
        assert_eq!(
            repo,
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
            }
        );

        // new repository
        let args = vec![
            "github".to_string(),
            "fioncat".to_string(),
            "dotfiles".to_string(),
        ];
        let selector = RepoSelector::from_args(&ctx, &args);
        let repo = selector.select_one(false, false).await.unwrap();
        assert_eq!(
            repo,
            Repository {
                remote: "github".to_string(),
                owner: "fioncat".to_string(),
                name: "dotfiles".to_string(),
                new_created: true,
                ..Default::default()
            }
        );
    }

    #[tokio::test]
    async fn test_select_one_name_local() {
        let ctx = context::tests::build_test_context("select_one_name_local");
        let args = vec![
            "github".to_string(),
            "fioncat".to_string(),
            "rox".to_string(),
        ];
        let selector = RepoSelector::from_args(&ctx, &args);
        let repo = selector.select_one(false, true).await.unwrap();
        assert_eq!(
            repo,
            Repository {
                remote: "github".to_string(),
                owner: "fioncat".to_string(),
                name: "roxide".to_string(),
                sync: true,
                pin: true,
                path: None,
                last_visited_at: 2234,
                visited_count: 20,
                new_created: false,
            }
        );

        // In local mode, the remote repository won't be selected
        let args = vec![
            "github".to_string(),
            "fioncat".to_string(),
            "dotfiles".to_string(),
        ];
        let selector = RepoSelector::from_args(&ctx, &args);
        let result = selector.select_one(false, true).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_select_many() {
        struct Case {
            args: Vec<&'static str>,
            opts: SelectManyReposOptions,
            expect_total: u32,
            expect_level: DisplayLevel,
            expect_repos: Vec<&'static str>,
        }

        let cases = [
            Case {
                args: vec![],
                opts: SelectManyReposOptions::default(),
                expect_total: 6,
                expect_level: DisplayLevel::Remote,
                expect_repos: vec![
                    "github:kubernetes/kubernetes",
                    "github:fioncat/nvimdots",
                    "github:fioncat/roxide",
                    "gitlab:fioncat/template",
                    "gitlab:fioncat/someproject",
                    "github:fioncat/otree",
                ],
            },
            Case {
                args: vec![],
                opts: SelectManyReposOptions {
                    sync: Some(true),
                    ..Default::default()
                },
                expect_total: 3,
                expect_level: DisplayLevel::Remote,
                expect_repos: vec![
                    "github:fioncat/nvimdots",
                    "github:fioncat/roxide",
                    "github:fioncat/otree",
                ],
            },
            Case {
                args: vec!["github"],
                opts: SelectManyReposOptions {
                    sync: Some(false),
                    ..Default::default()
                },
                expect_total: 1,
                expect_level: DisplayLevel::Owner,
                expect_repos: vec!["github:kubernetes/kubernetes"],
            },
            Case {
                args: vec![],
                opts: SelectManyReposOptions {
                    pin: Some(false),
                    ..Default::default()
                },
                expect_total: 2,
                expect_level: DisplayLevel::Remote,
                expect_repos: vec!["gitlab:fioncat/template", "gitlab:fioncat/someproject"],
            },
            Case {
                args: vec!["github"],
                opts: SelectManyReposOptions {
                    limit: Some(LimitOptions {
                        limit: 2,
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                expect_total: 4,
                expect_level: DisplayLevel::Owner,
                expect_repos: vec!["github:kubernetes/kubernetes", "github:fioncat/nvimdots"],
            },
            Case {
                args: vec!["github"],
                opts: SelectManyReposOptions {
                    limit: Some(LimitOptions {
                        offset: 1,
                        limit: 2,
                    }),
                    ..Default::default()
                },
                expect_total: 4,
                expect_level: DisplayLevel::Owner,
                expect_repos: vec!["github:fioncat/nvimdots", "github:fioncat/roxide"],
            },
            Case {
                args: vec!["github", "fioncat"],
                opts: SelectManyReposOptions::default(),
                expect_total: 3,
                expect_level: DisplayLevel::Name,
                expect_repos: vec![
                    "github:fioncat/nvimdots",
                    "github:fioncat/roxide",
                    "github:fioncat/otree",
                ],
            },
            Case {
                args: vec!["github", "fioncat", "non-exists"],
                opts: SelectManyReposOptions::default(),
                expect_total: 0,
                expect_level: DisplayLevel::Name,
                expect_repos: vec![],
            },
        ];

        let ctx = context::tests::build_test_context("select_many");
        for case in cases {
            let args = case.args.iter().map(|s| s.to_string()).collect::<Vec<_>>();
            let selector = RepoSelector::from_args(&ctx, &args);
            let list = selector.select_many(case.opts).unwrap();
            assert_eq!(list.total, case.expect_total);
            assert_eq!(list.level, case.expect_level);
            let names = list.items.iter().map(|r| r.full_name()).collect::<Vec<_>>();
            let expect_names = case
                .expect_repos
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>();
            assert_eq!(names, expect_names);
        }
    }

    #[derive(Debug)]
    enum CaseResult {
        Ok(&'static str, &'static str, &'static str),
        Err(String),
    }

    impl Default for CaseResult {
        fn default() -> Self {
            Self::Err("no expect set".to_string())
        }
    }

    #[derive(Debug, Default)]
    struct SelectRemoteCase {
        head: &'static str,
        owner: &'static str,
        name: &'static str,
        filter: Option<&'static str>,
        expect: CaseResult,
    }

    async fn run_select_remote_cases<I>(ctx: &ConfigContext, cases: I)
    where
        I: IntoIterator<Item = SelectRemoteCase>,
    {
        for case in cases {
            let mut args = vec![case.head.to_string()];
            if !case.owner.is_empty() {
                args.push(case.owner.to_string());
            }
            if !case.name.is_empty() {
                args.push(case.name.to_string());
            }
            let mut selector = RepoSelector::from_args(ctx, &args);
            if let Some(filter) = case.filter {
                selector.fzf_filter = Some(filter);
            }
            let result = selector.select_remote(false).await;
            match case.expect {
                CaseResult::Ok(expect_remote, expect_owner, expect_name) => {
                    let expect = Repository {
                        remote: expect_remote.to_string(),
                        owner: expect_owner.to_string(),
                        name: expect_name.to_string(),
                        new_created: true,
                        ..Default::default()
                    };
                    let repo = result.unwrap();
                    assert_eq!(repo, expect);
                }
                CaseResult::Err(expect) => {
                    let err = result.err().unwrap();
                    assert_eq!(err.to_string(), expect);
                }
            }
        }
    }

    #[tokio::test]
    async fn test_select_remote_by_url() {
        let cases = [
            SelectRemoteCase {
                head: "https://github.com/kubernetes/kubectl",
                expect: CaseResult::Ok("github", "kubernetes", "kubectl"),
                ..Default::default()
            },
            SelectRemoteCase {
                head: "https://github.com/fioncat/roxide",
                expect: CaseResult::Err(format!(
                    "repo {:?} already exists locally",
                    "github:fioncat/roxide"
                )),
                ..Default::default()
            },
            SelectRemoteCase {
                head: "github",
                expect: CaseResult::Err(
                    "the head must be url or ssh when selecting from remote".to_string(),
                ),
                ..Default::default()
            },
        ];

        let ctx = context::tests::build_test_context("select_remote_by_url");
        run_select_remote_cases(&ctx, cases).await;
    }

    #[tokio::test]
    async fn test_select_remote_by_owner() {
        let cases = [
            SelectRemoteCase {
                head: "github",
                owner: "fioncat",
                filter: Some("filewarden"),
                expect: CaseResult::Ok("github", "fioncat", "filewarden"),
                ..Default::default()
            },
            SelectRemoteCase {
                head: "github",
                owner: "fioncat",
                filter: Some("roxide"),
                expect: CaseResult::Err("fzf no match found".to_string()),
                ..Default::default()
            },
        ];

        let ctx = context::tests::build_test_context("select_remote_by_owner");
        run_select_remote_cases(&ctx, cases).await;
    }

    #[tokio::test]
    async fn test_select_remote_by_name() {
        let cases = [
            SelectRemoteCase {
                head: "github",
                owner: "fioncat",
                name: "filewarden",
                expect: CaseResult::Ok("github", "fioncat", "filewarden"),
                ..Default::default()
            },
            SelectRemoteCase {
                head: "github",
                owner: "fioncat",
                name: "roxide",
                expect: CaseResult::Err(format!(
                    "repo {:?} already exists locally",
                    "github:fioncat/roxide"
                )),
                ..Default::default()
            },
        ];

        let ctx = context::tests::build_test_context("select_remote_by_name");
        run_select_remote_cases(&ctx, cases).await;
    }

    #[test]
    fn test_select_remotes() {
        struct Case {
            limit: LimitOptions,
            expect: RemoteList,
        }

        let cases = [
            Case {
                limit: LimitOptions::default(),
                expect: RemoteList {
                    remotes: vec![
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
                    ],
                    total: 2,
                },
            },
            Case {
                limit: LimitOptions {
                    limit: 1,
                    offset: 0,
                },
                expect: RemoteList {
                    remotes: vec![RemoteState {
                        remote: "github".to_string(),
                        owner_count: 2,
                        repo_count: 4,
                    }],
                    total: 2,
                },
            },
        ];

        let ctx = context::tests::build_test_context("select_remotes");
        for case in cases {
            let list = select_remotes(&ctx, case.limit).unwrap();
            assert_eq!(list, case.expect);
        }
    }

    #[test]
    fn test_select_owners() {
        struct Case {
            remote: Option<String>,
            limit: LimitOptions,
            expect: OwnerList,
        }

        let cases = [
            Case {
                remote: None,
                limit: LimitOptions::default(),
                expect: OwnerList {
                    show_remote: true,
                    owners: vec![
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
                        OwnerState {
                            remote: "gitlab".to_string(),
                            owner: "fioncat".to_string(),
                            repo_count: 2,
                        },
                    ],
                    total: 3,
                },
            },
            Case {
                remote: None,
                limit: LimitOptions {
                    offset: 1,
                    limit: 2,
                },
                expect: OwnerList {
                    show_remote: true,
                    owners: vec![
                        OwnerState {
                            remote: "github".to_string(),
                            owner: "fioncat".to_string(),
                            repo_count: 3,
                        },
                        OwnerState {
                            remote: "gitlab".to_string(),
                            owner: "fioncat".to_string(),
                            repo_count: 2,
                        },
                    ],
                    total: 3,
                },
            },
            Case {
                remote: Some("github".to_string()),
                limit: LimitOptions::default(),
                expect: OwnerList {
                    show_remote: false,
                    owners: vec![
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
                    ],
                    total: 2,
                },
            },
        ];

        let ctx = context::tests::build_test_context("select_owners");
        for case in cases {
            let list = select_owners(&ctx, case.remote, case.limit).unwrap();
            assert_eq!(list, case.expect);
        }
    }
}

use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::{Result, bail};
use console::style;

use crate::config::context::ConfigContext;
use crate::config::hook::HookConfig;
use crate::config::remote::{OwnerConfigRef, RemoteConfig};
use crate::db::hook_history::HookHistory;
use crate::db::repo::Repository;
use crate::exec::git::GitCmd;
use crate::exec::git::branch::{Branch, BranchStatus};
use crate::exec::git::commit::{
    count_uncommitted_changes, ensure_no_uncommitted_changes, get_current_commit,
};
use crate::exec::git::remote::Remote;
use crate::format::now;
use crate::repo::hook::filter::matched_filters;
use crate::scan::code_stats::get_code_stats;
use crate::scan::ignore::Ignore;
use crate::{confirm, debug, info, outputln};

macro_rules! show_info {
    ($self:ident, $($arg:tt)*) => {
        if !$self.ctx.is_mute() {
            info!($($arg)*);
        }
    };
}

pub struct RepoOperator<'a, 'b> {
    ctx: &'a ConfigContext,
    remote: &'a RemoteConfig,
    owner: OwnerConfigRef<'a>,

    repo: &'b Repository,

    path: PathBuf,

    current_commit: OnceLock<Option<String>>,
    updated: OnceLock<Result<bool>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateResult {
    Created,
    Cloned,
    Exists,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HookResult<'a> {
    pub name: &'a str,
    pub success: bool,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct SyncResult {
    pub name: String,

    pub uncommitted: usize,

    pub pushed: Vec<String>,
    pub pulled: Vec<String>,
    pub deleted: Vec<String>,

    pub conflect: Vec<String>,
    pub detached: Vec<String>,
}

#[derive(Debug)]
struct BranchTask {
    branch: String,
    action: BranchAction,
}

#[derive(Debug)]
enum BranchAction {
    Push,
    Pull,
    Delete,
}

#[derive(Debug, Clone, Copy)]
pub struct RebaseOptions<'a> {
    pub target: &'a str,
    pub upstream: bool,
    pub force_no_cache: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct SquashOptions<'a> {
    pub target: &'a str,
    pub upstream: bool,
    pub force_no_cache: bool,
    pub message: &'a Option<String>,
}

impl<'a, 'b> RepoOperator<'a, 'b> {
    pub fn load(ctx: &'a ConfigContext, repo: &'b Repository) -> Result<Self> {
        let remote = ctx.cfg.get_remote(&repo.remote)?;
        let owner = remote.get_owner(&repo.owner);
        let path = repo.get_path(&ctx.cfg.workspace);
        debug!(
            "[op] Creating operator for repo {:?}, path: {:?}",
            repo.full_name(),
            path
        );
        Ok(Self::new(ctx, remote, owner, repo, path))
    }

    pub fn new(
        ctx: &'a ConfigContext,
        remote: &'a RemoteConfig,
        owner: OwnerConfigRef<'a>,
        repo: &'b Repository,
        path: PathBuf,
    ) -> Self {
        Self {
            ctx,
            remote,
            owner,
            repo,
            path,
            current_commit: OnceLock::new(),
            updated: OnceLock::new(),
        }
    }

    pub fn ensure_create(&self, thin: bool, clone_url: Option<String>) -> Result<CreateResult> {
        debug!("[op] Ensuring repo create");

        if self.path.exists() {
            debug!("[op] Repo already exists, return");
            return Ok(CreateResult::Exists);
        }

        let clone_url = match clone_url {
            Some(url) => Some(url),
            None => self.get_clone_url(),
        };
        debug!("[op] Clone URL: {clone_url:?}");
        let result = match clone_url {
            Some(url) => {
                debug!("[op] Cloning repo from {url:?}");
                let message = format!("Cloning from {url}");
                let path = format!("{}", self.path.display());
                let args = if thin {
                    vec!["clone", "--depth", "1", &url, &path]
                } else {
                    vec!["clone", &url, &path]
                };
                self.ctx.git().execute(args, message)?;

                self.ensure_user()?;
                CreateResult::Cloned
            }
            None => {
                debug!(
                    "[op] Creating empty repo, default branch: {}",
                    self.ctx.cfg.default_branch
                );

                show_info!(self, "Creating empty repository: {}", self.path.display());
                super::ensure_dir(&self.path)?;
                self.git().execute(
                    ["init", "-b", self.ctx.cfg.default_branch.as_str()],
                    "Initializing empty git repository",
                )?;
                CreateResult::Created
            }
        };

        debug!("[op] Ensure repo create done: {result:?}");
        Ok(result)
    }

    pub fn remove(&self) -> Result<()> {
        super::remove_dir_all(self.ctx, &self.path)
    }

    pub fn ensure_remote(&self) -> Result<()> {
        debug!("[op] Ensuring repo remote");

        let Some(url) = self.get_clone_url() else {
            debug!("[op] Remote does not support clone, skipping ensure_remote");
            return Ok(());
        };

        let Some(remote) = Remote::origin(self.git())? else {
            debug!("[op] Repo has no origin remote, adding: {url:?}");
            return self.git().execute(
                ["remote", "add", "origin", &url],
                format!("Adding origin remote: {url}"),
            );
        };

        let current_url = remote.get_url(self.git())?;
        if current_url == url {
            debug!("[op] Repo origin remote url is up-to-date: {url:?}");
            return Ok(());
        }

        debug!("[op] Repo origin remote url is different, current: {current_url:?}, new: {url:?}");
        self.git().execute(
            ["remote", "set-url", "origin", &url],
            format!("Setting origin remote: {url}"),
        )
    }

    pub fn ensure_user(&self) -> Result<()> {
        if let Some(ref user) = self.owner.user {
            debug!("[op] Setting user.name to {user:?}");
            let message = format!("Setting user to {user:?}");
            self.git().execute(["config", "user.name", user], message)?;
        }

        if let Some(ref email) = self.owner.email {
            debug!("[op] Setting user.email to {email:?}");
            let message = format!("Setting email to {email:?}");
            self.git()
                .execute(["config", "user.email", email], message)?;
        }

        Ok(())
    }

    pub async fn get_git_remote(&self, upstream: bool, force_no_cache: bool) -> Result<Remote> {
        debug!("[op] Getting git remote, upstream: {upstream}, force_no_cache: {force_no_cache}");
        if !upstream {
            let Some(remote) = Remote::origin(self.git())? else {
                bail!("repository does not have origin remote, please sync first");
            };
            debug!("[op] Getting origin remote: {remote:?}");
            return Ok(remote);
        }

        let remotes = Remote::list(self.git())?;
        for remote in remotes {
            if remote.as_str() == "upstream" {
                debug!("[op] Getting upstream remote: {remote:?}");
                return Ok(remote);
            }
        }

        debug!("[op] Upstream remote not found, trying to add it");
        let Some(ref domain) = self.remote.clone else {
            bail!(
                "remote {:?} does not support clone, cannot get upstream",
                self.repo.remote
            );
        };

        let api = self.ctx.get_api(&self.repo.remote, force_no_cache)?;

        show_info!(
            self,
            "Getting upstream info for repo {}",
            self.repo.full_name()
        );
        let api_repo = api
            .get_repo(&self.repo.remote, &self.repo.owner, &self.repo.name)
            .await?;
        debug!("[op] API repo info: {api_repo:?}");
        let Some(upstream) = api_repo.upstream else {
            bail!("repo is not forked and without an upstream");
        };

        let upstream_owner = self.remote.get_owner(&upstream.owner);
        let upstream_url = Repository::get_clone_url_raw(
            domain,
            &upstream.owner,
            &upstream.name,
            upstream_owner.ssh,
        );
        debug!("[op] Upstream url: {upstream_url:?}");

        if !self.ctx.is_mute() {
            confirm!(
                "Do you want to set upstream to {}/{}: {upstream_url}",
                upstream.owner,
                upstream.name
            );
        }

        show_info!(self, "Setting upstream remote to {upstream_url:?}");
        self.git().execute(
            ["remote", "add", "upstream", &upstream_url],
            format!("Setting upstream to {upstream_url}"),
        )?;

        let remote = Remote::new("upstream");
        debug!("[op] Adding upstream remote: {remote:?}");
        Ok(remote)
    }

    pub fn sync(&self) -> Result<SyncResult> {
        debug!("[op] Beginning to sync repo");
        let result = self.ensure_create(false, None)?;
        if !matches!(result, CreateResult::Cloned) {
            debug!("[op] Repo not cloned, ensuring user and remote");
            self.ensure_user()?;
            self.ensure_remote()?;
        }

        let mut result = SyncResult {
            name: self.repo.full_name(),
            ..Default::default()
        };

        self.git().execute(
            ["fetch", "origin", "--prune", "--prune-tags"],
            "Fetching origin remote",
        )?;

        let uncommitted = count_uncommitted_changes(self.git())?;
        if uncommitted > 0 {
            debug!("[op] Repo has {uncommitted} uncommitted changes, skipping sync branches");
            result.uncommitted = uncommitted;
            return Ok(result);
        }

        let branches = Branch::list(self.git())?;
        let default_branch = Branch::default(self.git())?;
        let mut back = default_branch.clone();
        debug!(
            "[op] Beginning to sync branches, default_branch: {default_branch}, branches: {branches:?}"
        );

        let mut tasks = vec![];
        let mut current = String::new();
        for branch in branches {
            if branch.current {
                current = branch.name.clone();
                if !matches!(branch.status, BranchStatus::Gone) {
                    // If the current branch is not gone, we need to checkout back it after
                    // all tasks done.
                    // If it is gone, checkout to the default branch.
                    back = branch.name.clone();
                }
            }
            let task = match branch.status {
                BranchStatus::Ahead => BranchTask {
                    branch: branch.name,
                    action: BranchAction::Push,
                },
                BranchStatus::Behind => BranchTask {
                    branch: branch.name,
                    action: BranchAction::Pull,
                },
                BranchStatus::Gone => {
                    if branch.name == default_branch {
                        // We cannot delete the default branch
                        continue;
                    }
                    BranchTask {
                        branch: branch.name,
                        action: BranchAction::Delete,
                    }
                }
                BranchStatus::Conflict => {
                    // We cannot handle the conflict automatically, leave it to user
                    result.conflect.push(branch.name);
                    continue;
                }
                BranchStatus::Detached => {
                    // We cannot handle the detached branch automatically, leave it to user
                    result.detached.push(branch.name);
                    continue;
                }
                BranchStatus::Sync => continue,
            };
            tasks.push(task);
        }

        debug!("[op] Syncing branch tasks: {tasks:?}, back: {back}, current: {current}");

        if tasks.is_empty() {
            debug!("[op] No branch to sync");
            show_info!(self, "No branch to sync");
            return Ok(result);
        }

        show_info!(self, "Backup branch is {}", style(&back).magenta().bold());

        for task in tasks {
            debug!("[op] Beginning to handle sync branch task: {task:?}");
            match task.action {
                BranchAction::Push | BranchAction::Pull => {
                    if current != task.branch {
                        debug!("[op] Checkouting to branch {} to push or pull", task.branch);
                        // checkout to this branch to perform push/pull
                        self.git().execute(
                            ["checkout", &task.branch],
                            format!("Checkouting to branch {}", task.branch),
                        )?;
                        current = task.branch.clone();
                    }

                    let (title, op) = match task.action {
                        BranchAction::Push => ("Pushing", "push"),
                        BranchAction::Pull => ("Pulling", "pull"),
                        _ => unreachable!(),
                    };
                    debug!("[op] {title} branch {}", task.branch);
                    self.git().execute(
                        [op, "origin", &task.branch],
                        format!("{title} branch {}", task.branch),
                    )?;
                }
                BranchAction::Delete => {
                    if current == task.branch {
                        debug!("[op] Checkouting to default branch {default_branch} before delete");
                        // we cannot delete branch when we are inside it, checkout
                        // to default branch first.
                        self.git().execute(
                            ["checkout", &default_branch],
                            format!("Checkouting to default branch {default_branch}"),
                        )?;
                    }

                    debug!("[op] Deleting branch {}", task.branch);
                    self.git().execute(
                        ["branch", "-D", &task.branch],
                        format!("Deleting branch {}", task.branch),
                    )?;
                }
            }
            match task.action {
                BranchAction::Push => result.pushed.push(task.branch),
                BranchAction::Pull => result.pulled.push(task.branch),
                BranchAction::Delete => result.deleted.push(task.branch),
            }
        }

        if current != back {
            debug!("[op] Checkouting to backup branch {back:?}");
            self.git().execute(
                ["checkout", &back],
                format!("Checkouting to backup branch {back}"),
            )?;
        }

        debug!("[op] Sync branches done, result: {result:?}");
        Ok(result)
    }

    pub async fn rebase(&self, opts: RebaseOptions<'_>) -> Result<()> {
        debug!("[op] Beginning to rebase repo, options: {opts:?}");
        ensure_no_uncommitted_changes(self.git())?;

        let remote = self
            .get_git_remote(opts.upstream, opts.force_no_cache)
            .await?;
        debug!("[op] Getting remote for rebase: {remote:?}");

        let target = remote.get_target(self.git(), opts.target)?;
        debug!("[op] Getting target for rebase: {target:?}");

        self.git()
            .execute(["rebase", &target], format!("Rebasing from {target}"))
    }

    pub async fn squash(&self, opts: SquashOptions<'_>) -> Result<()> {
        debug!("[op] Beginning to squash repo, options: {opts:?}");
        ensure_no_uncommitted_changes(self.git())?;

        let remote = self
            .get_git_remote(opts.upstream, opts.force_no_cache)
            .await?;
        debug!("[op] Getting remote for squash: {remote:?}");

        let commits = remote.commits_between(self.git(), opts.target, true)?;
        debug!("[op] Commits between: {commits:?}");

        if commits.is_empty() {
            debug!("[op] No new commits to squash");
            show_info!(self, "No new commits to squash");
            return Ok(());
        }

        if commits.len() == 1 {
            debug!("[op] Only one commit, no need to squash");
            show_info!(self, "Only one commit, no need to squash");
            return Ok(());
        }

        if !self.ctx.is_mute() {
            info!("Found {} commits to squash:", commits.len());
            for commit in commits.iter() {
                outputln!("  * {commit}");
            }
            confirm!("Continue");
        }

        debug!("[op] Soft resetting to squash commits");
        let set = format!("HEAD~{}", commits.len());
        self.git()
            .execute(["reset", "--soft", &set], "Soft resetting to squash")?;

        debug!("[op] Committing squashed changes");
        let args = if let Some(message) = opts.message {
            vec!["commit", "--message", message.as_str()]
        } else {
            vec!["commit"]
        };
        self.git().execute(args, "Committing squashed changes")
    }

    pub fn run_hooks<'this>(
        &'this self,
        create_result: CreateResult,
    ) -> Result<Vec<HookResult<'this>>> {
        debug!("[op] Running hooks for repo {:?}", self.repo);
        let mut to_run = vec![];
        for hook in self.ctx.cfg.hooks.iter() {
            debug!("[op] Checking hook: {hook:?}");
            if !matched_filters(self.repo, &hook.filters) {
                debug!("[op] Hook not matched filter, skipping");
                continue;
            }
            let mut should_run = false;
            for cond in hook.conditions.iter() {
                if cond.matched(self, &hook.name, create_result)? {
                    debug!("[op] Hook condition matched: {cond:?}");
                    should_run = true;
                    break;
                }
            }
            if should_run {
                debug!("[op] Hook will be run");
                to_run.push(hook);
            } else {
                debug!("[op] Hook conditions not matched, skipping");
            }
        }

        if to_run.is_empty() {
            debug!("[op] No hook to run");
            return Ok(vec![]);
        }

        let mut results = vec![];
        let envs = self.build_hook_envs();
        debug!("[op] Running hooks with envs: {envs:?}");
        for hook in to_run {
            let success = self.run_hook(hook, &envs)?;
            results.push(HookResult {
                name: &hook.name,
                success,
            });
        }
        Ok(results)
    }

    pub fn run_hook(&self, hook: &HookConfig, envs: &[(&'static str, Cow<str>)]) -> Result<bool> {
        show_info!(self, "Running hook: {}", style(&hook.name).magenta().bold());
        debug!(
            "[op] Running hook for repo {:?}, hook config: {hook:?}",
            self.repo
        );
        let mut success = true;
        for run in hook.run.iter() {
            debug!("[op] Running hook run: {run:?}");
            let Some(run_path) = self.ctx.cfg.hook_runs.get(run) else {
                debug!("[op] Hook run {run:?} not found, skipping");
                continue;
            };

            let result = self
                .ctx
                .run_bash(&self.path, run_path, envs, format!("Running: {run}"));
            if result.is_err() {
                debug!("[op] Hook run {run:?} failed: {result:?}");
                success = false;
                break;
            }
        }

        let history = HookHistory {
            id: 0,
            repo_id: self.repo.id,
            name: hook.name.clone(),
            success,
            time: now(),
        };
        let db = self.ctx.get_db()?;
        db.with_transaction(
            |tx| match tx.hook_history().get(history.repo_id, &history.name)? {
                Some(_) => tx.hook_history().update(&history),
                None => {
                    tx.hook_history().insert(&history)?;
                    Ok(())
                }
            },
        )?;

        if success {
            show_info!(self, "Hook {} {}", hook.name, style("ok").green());
        } else {
            show_info!(self, "Hook {} {}", hook.name, style("failed").red());
        }
        Ok(success)
    }

    pub fn build_hook_envs<'this>(&'this self) -> [(&'static str, Cow<'this, str>); 8] {
        let current_branch = Branch::current(self.git().mute()).unwrap_or_default();
        let default_branch = Branch::default(self.git().mute()).unwrap_or_default();
        [
            ("REMOTE_NAME", Cow::Borrowed(&self.repo.remote)),
            (
                "REMOTE_CLONE",
                Cow::Borrowed(self.remote.clone.as_deref().unwrap_or_default()),
            ),
            ("OWNER_NAME", Cow::Borrowed(&self.repo.owner)),
            (
                "OWNER_USER",
                Cow::Borrowed(self.owner.user.unwrap_or_default()),
            ),
            (
                "OWNER_EMAIL",
                Cow::Borrowed(self.owner.email.unwrap_or_default()),
            ),
            ("REPO_NAME", Cow::Borrowed(&self.repo.name)),
            ("CURRENT_BRANCH", current_branch.into()),
            ("DEFAULT_BRANCH", default_branch.into()),
        ]
    }

    pub fn get_current_commit(&self) -> Option<&str> {
        debug!("[op] Getting current commit");
        self.current_commit
            .get_or_init(|| {
                let commit = get_current_commit(self.git().mute()).ok();
                debug!("[op] Current commit: {commit:?}");
                commit
            })
            .as_deref()
    }

    pub fn is_updated(&self) -> Result<bool> {
        debug!("[op] Checking if repo is updated");
        let updated = self.updated.get_or_init(|| {
            if count_uncommitted_changes(self.git().mute())? > 0 {
                debug!("[op] Repo has uncommitted changes, updated");
                return Ok(true);
            }
            if self.repo.commit.as_deref() != self.get_current_commit() {
                debug!(
                    "[op] Repo current commit {:?} is different from current commit {:?}, updated",
                    self.repo.commit,
                    self.get_current_commit()
                );
                return Ok(true);
            }
            debug!("[op] Repo is not updated");
            Ok(false)
        });
        match updated {
            Ok(v) => Ok(*v),
            Err(e) => bail!(e.to_string()),
        }
    }

    pub async fn update_language(&self, force: bool) -> Result<()> {
        debug!("[op] Updating language");
        let db = self.ctx.get_db()?;
        if let Some(language) = self.detect(force).await? {
            if self.repo.language.as_deref() != language {
                debug!("[op] Updating language to {language:?}");
                show_info!(
                    self,
                    "Updating language to {}",
                    language
                        .map(|lang| format!("{lang:?}").into())
                        .unwrap_or(Cow::Borrowed("<none>"))
                );
                db.with_transaction(|tx| tx.repo().update_language(self.repo.id, language))?;
            } else {
                debug!("[op] Language is already up to date, skipping update");
            }
        } else {
            debug_assert!(!force); // When force is true, detect should always returns Some
            debug!("[op] Auto detect disabled or no update needed, skipping update");
        }

        Ok(())
    }

    async fn detect(&self, force: bool) -> Result<Option<Option<&'static str>>> {
        debug!("[op] Detecting language");
        if !force {
            if self.ctx.cfg.disable_auto_detect {
                debug!("[op] Auto detect is disabled, skipping");
                return Ok(None);
            }
            if !self.is_updated()? {
                debug!("[op] Repo is not updated, skipping detect");
                return Ok(None);
            }
        }

        let ignore = if self.ctx.cfg.stats_ignore.is_empty() {
            Ignore::default()
        } else {
            Ignore::parse(&self.path, &self.ctx.cfg.stats_ignore)?
        };

        let mut stats = get_code_stats(self.path.clone(), ignore).await?;
        if stats.items.is_empty() {
            debug!("[op] No code stats found, setting language to None");
            return Ok(Some(None));
        }

        if let Some(pos) = stats.items.iter().position(|item| item.main) {
            let top_item = stats.items.remove(pos);
            debug!("[op] Main code stats item: {top_item:?}");
            return Ok(Some(Some(top_item.name)));
        }

        let top_item = stats.items.remove(0);
        debug!("[op] No main top code stats item: {top_item:?}");
        Ok(Some(Some(top_item.name)))
    }

    pub fn update_commit(&self) -> Result<()> {
        debug!("[op] Updating repo commit");
        let db = self.ctx.get_db()?;
        let current_commit = self.get_current_commit();
        if self.repo.commit.as_deref() != current_commit {
            debug!(
                "[op] Repo current commit {:?} is different from current commit {:?}, updating",
                self.repo.commit, current_commit
            );
            db.with_transaction(|tx| tx.repo().update_commit(self.repo.id, current_commit))?;
        } else {
            debug!("[op] Repo commit is already up to date, skipping update");
        }
        Ok(())
    }

    pub fn ctx(&self) -> &ConfigContext {
        self.ctx
    }

    pub fn repo(&self) -> &Repository {
        self.repo
    }

    pub fn get_clone_url(&self) -> Option<String> {
        let domain = self.remote.clone.as_ref()?;
        Some(self.repo.get_clone_url(domain, self.owner.ssh))
    }

    pub fn path(&self) -> &Path {
        self.path.as_ref()
    }

    pub fn remote(&self) -> &RemoteConfig {
        self.remote
    }

    pub fn owner(&self) -> OwnerConfigRef<'a> {
        self.owner
    }

    #[inline]
    pub fn git<'this>(&'this self) -> GitCmd<'this> {
        self.ctx.git_work_dir(&self.path)
    }
}

impl SyncResult {
    pub fn render(&self, with_header: bool) -> String {
        let mut fields = vec![];
        if self.uncommitted > 0 {
            let flag = style("*").yellow().bold();
            let field = format!("  {flag} {} dirty", self.uncommitted);
            fields.push(field);
        }

        if !self.pushed.is_empty() {
            let flag = style("↑").green().bold();
            let field = format!("  {flag} {}", self.pushed.join(", "));
            fields.push(field);
        }

        if !self.pulled.is_empty() {
            let flag = style("↓").green().bold();
            let field = format!("  {flag} {}", self.pulled.join(", "));
            fields.push(field);
        }

        if !self.deleted.is_empty() {
            let flag = style("-").red().bold();
            let field = format!("  {flag} {}", self.deleted.join(", "));
            fields.push(field);
        }

        if !self.conflect.is_empty() {
            let flag = style("$").red().bold();
            let field = format!("  {flag} {}", self.conflect.join(", "));
            fields.push(field);
        }

        if !self.detached.is_empty() {
            let flag = style("?").red().bold();
            let field = format!("  {flag} {}", self.detached.join(", "));
            fields.push(field);
        }

        if fields.is_empty() {
            return String::new();
        }

        let mut result = if with_header {
            format!("> {}:\n", self.name)
        } else {
            String::new()
        };
        result.push_str(&fields.join("\n"));
        result
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use crate::config::context;
    use crate::exec::git;
    use crate::repo::select::RepoSelector;

    use super::*;

    #[tokio::test]
    async fn test_create_clone() {
        if !git::tests::enable() {
            return;
        }

        let ctx = context::tests::build_test_context("create_clone");
        let args = ["roxide".to_string()];
        let selector = RepoSelector::from_args(&ctx, &args);
        let repo = selector.select_one(false, true).await.unwrap();
        let op = RepoOperator::load(&ctx, &repo).unwrap();
        op.ensure_create(false, None).unwrap();

        let git_remote = git::remote::Remote::origin(op.git()).unwrap().unwrap();
        assert_eq!(git_remote.as_str(), "origin");

        let url = git_remote.get_url(op.git()).unwrap();
        let expect = op.get_clone_url().unwrap();
        assert_eq!(url, expect);

        // The create can be called multiple times
        op.ensure_create(false, None).unwrap();
    }

    #[test]
    fn test_create_empty() {
        let repo = Repository {
            remote: "test".to_string(),
            owner: "rust".to_string(),
            name: "hello".to_string(),
            ..Default::default()
        };
        let ctx = context::tests::build_test_context("create_empty");
        let op = RepoOperator::load(&ctx, &repo).unwrap();
        op.ensure_create(true, None).unwrap();

        op.git().execute(["status"], "").unwrap();
    }

    #[test]
    fn test_remove() {
        let repo = Repository {
            remote: "test".to_string(),
            owner: "rust".to_string(),
            name: "hello".to_string(),
            ..Default::default()
        };
        let ctx = context::tests::build_test_context("remove");
        let path = repo.get_path(&ctx.cfg.workspace);
        let op = RepoOperator::load(&ctx, &repo).unwrap();
        op.ensure_create(true, None).unwrap();

        fs::write(Path::new(&ctx.cfg.workspace).join("test.txt"), "test").unwrap();

        assert!(path.exists());
        op.remove().unwrap();
        assert!(!path.exists());
        // The workspace should be kept, because we wrote a file in it
        assert!(Path::new(&ctx.cfg.workspace).exists());
    }

    #[tokio::test]
    async fn test_get_git_remote() {
        if !git::tests::enable() {
            return;
        }

        let ctx = context::tests::build_test_context("get_git_remote");
        let repo = Repository {
            remote: "github".to_string(),
            owner: "fioncat".to_string(),
            name: "example".to_string(),
            ..Default::default()
        };
        let op = RepoOperator::load(&ctx, &repo).unwrap();
        op.ensure_create(true, None).unwrap();

        let path = repo.get_path(&ctx.cfg.workspace);
        assert!(path.exists());

        let remote = op.get_git_remote(false, false).await.unwrap();
        assert_eq!(remote.as_str(), "origin");
    }

    #[tokio::test]
    async fn test_get_git_remote_upstream() {
        if !git::tests::enable() {
            return;
        }

        let ctx = context::tests::build_test_context("get_git_remote_upstream");
        let repo = Repository {
            remote: "github".to_string(),
            owner: "fioncat".to_string(),
            name: "nvimdots".to_string(),
            ..Default::default()
        };
        let op = RepoOperator::load(&ctx, &repo).unwrap();
        op.ensure_create(true, None).unwrap();

        let path = repo.get_path(&ctx.cfg.workspace);
        assert!(path.exists());

        let remote = op.get_git_remote(true, true).await.unwrap();
        assert_eq!(remote.as_str(), "upstream");

        let url = remote.get_url(op.git()).unwrap();
        assert_eq!(url, "https://github.com/ayamir/nvimdots.git");
    }

    #[test]
    fn test_sync_branch() {
        if !git::tests::enable() {
            return;
        }

        let ctx = context::tests::build_test_context("sync_branch");
        let repo = Repository {
            remote: "github".to_string(),
            owner: "fioncat".to_string(),
            name: "example".to_string(),
            ..Default::default()
        };
        let op = RepoOperator::load(&ctx, &repo).unwrap();
        op.ensure_create(false, None).unwrap();

        let path = repo.get_path(&ctx.cfg.workspace);
        assert!(path.exists());

        // Reset a commit, to test pulling
        op.git()
            .execute(["reset", "--hard", "HEAD~1"], "Resetting last commit")
            .unwrap();

        // Create a new branch, to test pushing
        op.git()
            .execute(["checkout", "-b", "test-push"], "Creating new branch")
            .unwrap();
        op.git()
            .execute(
                ["push", "origin", "--set-upstream", "test-push"],
                "Pushing new branch",
            )
            .unwrap();
        fs::write(path.join("test.txt"), "test").unwrap();
        op.git().execute(["add", "."], "Adding file").unwrap();
        op.git()
            .execute(["commit", "-m", "Adding test file"], "Committing file")
            .unwrap();

        op.git()
            .execute(
                ["checkout", "-b", "test-detached"],
                "Creating detached branch",
            )
            .unwrap();

        let branches = Branch::list(op.git()).unwrap();
        let push_branch = branches.iter().find(|b| b.name == "test-push").unwrap();
        let pull_branch = branches.iter().find(|b| b.name == "master").unwrap();
        let detached_branch = branches.iter().find(|b| b.name == "test-detached").unwrap();
        assert_eq!(push_branch.status, BranchStatus::Ahead);
        assert_eq!(pull_branch.status, BranchStatus::Behind);
        assert_eq!(detached_branch.status, BranchStatus::Detached);

        let result = op.sync().unwrap();
        assert_eq!(
            result,
            SyncResult {
                name: "github:fioncat/example".to_string(),
                pushed: vec!["test-push".to_string()],
                pulled: vec!["master".to_string()],
                detached: vec!["test-detached".to_string()],
                ..Default::default()
            }
        );

        let branches = Branch::list(op.git()).unwrap();
        let push_branch = branches.iter().find(|b| b.name == "test-push").unwrap();
        let pull_branch = branches.iter().find(|b| b.name == "master").unwrap();
        let detached_branch = branches.iter().find(|b| b.name == "test-detached").unwrap();
        assert_eq!(push_branch.status, BranchStatus::Sync);
        assert_eq!(pull_branch.status, BranchStatus::Sync);
        assert_eq!(detached_branch.status, BranchStatus::Detached);

        let current = branches.iter().find(|b| b.current).unwrap();
        assert_eq!(current.name, "test-detached");

        // Cleanup
        op.git()
            .execute(["branch", "-D", "test-push"], "Delete test-push branch")
            .unwrap();
        op.git()
            .execute(
                ["push", "origin", "--delete", "test-push"],
                "Delete remote test-push branch",
            )
            .unwrap();
    }

    #[test]
    fn test_sync_uncommitted() {
        if !git::tests::enable() {
            return;
        }

        let ctx = context::tests::build_test_context("sync_uncommitted");
        let repo = Repository {
            remote: "github".to_string(),
            owner: "fioncat".to_string(),
            name: "example".to_string(),
            ..Default::default()
        };
        let op = RepoOperator::load(&ctx, &repo).unwrap();
        op.ensure_create(true, None).unwrap();

        let path = repo.get_path(&ctx.cfg.workspace);
        assert!(path.exists());

        fs::write(path.join("test0.txt"), "test").unwrap();
        fs::write(path.join("test1.txt"), "test").unwrap();
        fs::write(path.join("test2.txt"), "test").unwrap();

        let result = op.sync().unwrap();
        assert_eq!(
            result,
            SyncResult {
                name: "github:fioncat/example".to_string(),
                uncommitted: 3,
                ..Default::default()
            }
        );
    }

    #[tokio::test]
    async fn test_sync_create() {
        if !git::tests::enable() {
            return;
        }

        let ctx = context::tests::build_test_context("sync_create");
        let repo = Repository {
            remote: "github".to_string(),
            owner: "fioncat".to_string(),
            name: "example".to_string(),
            ..Default::default()
        };
        let op = RepoOperator::load(&ctx, &repo).unwrap();

        let path = repo.get_path(&ctx.cfg.workspace);
        assert!(!path.exists());

        let result = op.sync().unwrap();
        assert_eq!(
            result,
            SyncResult {
                name: "github:fioncat/example".to_string(),
                ..Default::default()
            }
        );

        assert!(path.exists());

        let remote = op.get_git_remote(false, false).await.unwrap();
        assert_eq!(remote.as_str(), "origin");
    }

    #[tokio::test]
    async fn test_sync_ensure() {
        if !git::tests::enable() {
            return;
        }
        let ctx = context::tests::build_test_context("sync_ensure");
        let repo = Repository {
            remote: "github".to_string(),
            owner: "fioncat".to_string(),
            name: "example".to_string(),
            ..Default::default()
        };
        let op = RepoOperator::load(&ctx, &repo).unwrap();
        op.ensure_create(true, None).unwrap();

        let path = repo.get_path(&ctx.cfg.workspace);
        assert!(path.exists());

        op.git()
            .execute(["config", "user.name", "test-user"], "Setting user name")
            .unwrap();
        op.git()
            .execute(
                ["config", "user.email", "test-email@test.com"],
                "Setting user email",
            )
            .unwrap();
        op.git()
            .execute(
                [
                    "remote",
                    "set-url",
                    "origin",
                    "https://github.com/fioncat/roxide.git",
                ],
                "Updating origin remote",
            )
            .unwrap();

        let result = op.sync().unwrap();
        assert_eq!(
            result,
            SyncResult {
                name: "github:fioncat/example".to_string(),
                ..Default::default()
            }
        );

        let name = op.git().output(["config", "user.name"], "").unwrap();
        let email = op.git().output(["config", "user.email"], "").unwrap();
        assert_eq!(name, "fioncat");
        assert_eq!(email, "lazycat7706@gmail.com");

        let remote = op.get_git_remote(false, false).await.unwrap();
        assert_eq!(remote.as_str(), "origin");
        let url = remote.get_url(op.git()).unwrap();
        assert_eq!(url, op.get_clone_url().unwrap());
    }

    #[tokio::test]
    async fn test_rebase() {
        if !git::tests::enable() {
            return;
        }
        let ctx = context::tests::build_test_context("rebase");
        let repo = Repository {
            remote: "github".to_string(),
            owner: "fioncat".to_string(),
            name: "example".to_string(),
            ..Default::default()
        };
        let op = RepoOperator::load(&ctx, &repo).unwrap();
        op.ensure_create(false, None).unwrap();

        let path = repo.get_path(&ctx.cfg.workspace);
        assert!(path.exists());

        op.git()
            .execute(
                ["checkout", "-b", "test-rebase-target"],
                "Creating rebase target branch",
            )
            .unwrap();

        fs::write(path.join("test_target.txt"), "content from target branch").unwrap();
        op.git().execute(["add", "."], "Adding file").unwrap();
        op.git()
            .execute(["commit", "-m", "Add test_target.txt"], "Committing file")
            .unwrap();
        op.git()
            .execute(
                ["push", "origin", "test-rebase-target"],
                "Pushing target branch",
            )
            .unwrap();

        op.git()
            .execute(["checkout", "master"], "Checkouting back to master")
            .unwrap();

        op.git()
            .execute(
                ["checkout", "-b", "test-rebase"],
                "Creating test-rebase branch",
            )
            .unwrap();
        fs::write(path.join("test_rebase.txt"), "content from rebase branch").unwrap();
        op.git().execute(["add", "."], "Adding file").unwrap();
        op.git()
            .execute(["commit", "-m", "Add test_rebase.txt"], "Committing file")
            .unwrap();

        let target_path = path.join("test_target.txt");
        let rebase_path = path.join("test_rebase.txt");

        assert!(!target_path.exists());
        assert!(rebase_path.exists());

        op.rebase(RebaseOptions {
            target: "test-rebase-target",
            upstream: false,
            force_no_cache: false,
        })
        .await
        .unwrap();

        assert!(target_path.exists());
        assert!(rebase_path.exists());

        op.git()
            .execute(
                ["branch", "-D", "test-rebase-target"],
                "Deleting test-rebase branch",
            )
            .unwrap();
        op.git()
            .execute(
                ["push", "origin", "--delete", "test-rebase-target"],
                "Deleting remote test-rebase branch",
            )
            .unwrap();
    }

    #[tokio::test]
    async fn test_squash() {
        if !git::tests::enable() {
            return;
        }
        let ctx = context::tests::build_test_context("squash");
        let repo = Repository {
            remote: "github".to_string(),
            owner: "fioncat".to_string(),
            name: "example".to_string(),
            ..Default::default()
        };
        let op = RepoOperator::load(&ctx, &repo).unwrap();
        op.ensure_create(false, None).unwrap();

        let path = repo.get_path(&ctx.cfg.workspace);
        assert!(path.exists());

        op.git()
            .execute(
                ["checkout", "-b", "test-squash-target"],
                "Creating squash target branch",
            )
            .unwrap();

        let message = Some("Squashed commit".to_string());
        let opts = SquashOptions {
            target: "",
            message: &message,
            upstream: false,
            force_no_cache: false,
        };

        fs::write(path.join("test1.txt"), "Test content 1").unwrap();
        op.git().execute(["add", "."], "Adding file").unwrap();
        op.git()
            .execute(["commit", "-m", "Add test1.txt"], "Committing file")
            .unwrap();

        fs::write(path.join("test2.txt"), "Test content 2").unwrap();
        op.git().execute(["add", "."], "Adding file").unwrap();
        op.git()
            .execute(["commit", "-m", "Add test2.txt"], "Committing file")
            .unwrap();

        op.squash(opts).await.unwrap();

        let lines = op
            .git()
            .lines(
                [
                    "log",
                    "--left-right",
                    "--cherry-pick",
                    "--oneline",
                    "HEAD...origin/master",
                ],
                "Getting commits",
            )
            .unwrap();
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn test_run_hooks() {
        let ctx = context::tests::build_test_context("run_hooks");
        let repo = Repository {
            remote: "test".to_string(),
            owner: "rust".to_string(),
            name: "hello".to_string(),
            ..Default::default()
        };
        let op = RepoOperator::load(&ctx, &repo).unwrap();
        op.ensure_create(true, None).unwrap();
        op.run_hooks(CreateResult::Created).unwrap();

        // The cargo init should be run
        let path = op.path().join("Cargo.toml");
        assert!(path.exists());
    }

    #[test]
    fn test_run_hook() {
        let ctx = context::tests::build_test_context("run_hook");
        let repo = Repository {
            remote: "test".to_string(),
            owner: "rust".to_string(),
            name: "hello".to_string(),
            ..Default::default()
        };
        let op = RepoOperator::load(&ctx, &repo).unwrap();
        op.ensure_create(true, None).unwrap();

        let hook = ctx.cfg.get_hook("print-envs").unwrap();
        let envs = op.build_hook_envs();
        op.run_hook(hook, &envs).unwrap();

        let path = op.path().join("repo.txt");
        let content = fs::read_to_string(path).unwrap();
        assert_eq!(content.trim(), repo.full_name());

        let path = op.path().join("branch.txt");
        let content = fs::read_to_string(path).unwrap();
        assert_eq!(content.trim(), ctx.cfg.default_branch);
    }
}

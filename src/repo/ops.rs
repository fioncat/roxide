use std::ffi::OsStr;
use std::path::PathBuf;
use std::{fs, io};

use anyhow::{Context, Result, bail};
use console::style;

use crate::config::context::ConfigContext;
use crate::config::remote::{OwnerConfigRef, RemoteConfig};
use crate::db::repo::Repository;
use crate::exec::git::branch::{Branch, BranchStatus};
use crate::exec::git::commit::{count_uncommitted_changes, ensure_no_uncommitted_changes};
use crate::exec::git::remote::Remote;
use crate::exec::{bash, git};
use crate::{confirm, debug, info, outputln};

macro_rules! show_info {
    ($self:ident, $($arg:tt)*) => {
        if !$self.mute {
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

    mute: bool,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct SyncResult {
    pub name: String,

    pub uncomitted: usize,

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
    target: &'a str,
    upstream: bool,
    force_no_cache: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct SquashOptions<'a> {
    target: &'a str,
    upstream: bool,
    force_no_cache: bool,
    message: &'a Option<String>,
}

impl<'a, 'b> RepoOperator<'a, 'b> {
    pub fn new(ctx: &'a ConfigContext, repo: &'b Repository, mute: bool) -> Result<Self> {
        let remote = ctx.cfg.get_remote(&repo.remote)?;
        let owner = remote.get_owner(&repo.owner);
        let path = repo.get_path(&ctx.cfg.workspace);
        debug!(
            "[op] Create operator for repo {:?}, path: {:?}",
            repo.full_name(),
            path
        );
        Ok(Self {
            ctx,
            remote,
            owner,
            repo,
            path,
            mute,
        })
    }

    pub fn ensure_create(&self, thin: bool, clone_url: Option<String>) -> Result<bool> {
        debug!("[op] Ensure repo create");

        if self.path.exists() {
            debug!("[op] Repo already exists, return");
            return Ok(false);
        }

        let clone_url = match clone_url {
            Some(url) => Some(url),
            None => self.get_clone_url(),
        };
        debug!("[op] Clone URL: {clone_url:?}");
        let mut cloned = false;

        match clone_url {
            Some(url) => {
                debug!("[op] Clone repo from {url:?}");
                let message = format!("Cloning from {url}");
                let path = format!("{}", self.path.display());
                let args = if thin {
                    vec!["clone", "--depth", "1", &url, &path]
                } else {
                    vec!["clone", &url, &path]
                };
                git::new(args, None::<&str>, message, self.mute).execute()?;

                self.ensure_user()?;
                cloned = true;
            }
            None => {
                debug!(
                    "[op] Create empty repo, default branch: {}",
                    self.ctx.cfg.default_branch
                );

                show_info!(self, "Create empty repository: {}", self.path.display());
                super::ensure_dir(&self.path)?;
                self.git(
                    ["init", "-b", self.ctx.cfg.default_branch.as_str()],
                    "Initializing empty git repository",
                )?;
            }
        };

        if !self.owner.on_create.is_empty() {
            let envs = [
                ("REPO_REMOTE", self.repo.remote.as_str()),
                ("REPO_NAME", self.repo.name.as_str()),
                ("REPO_OWNER", self.repo.owner.as_str()),
            ];

            for hook_name in self.owner.on_create.iter() {
                debug!("[op] Run create hook: {hook_name:?}");
                let Some(hook_path) = self.ctx.cfg.hooks.get(hook_name) else {
                    bail!("hook {hook_name:?} not found");
                };

                bash::run(
                    &self.path,
                    hook_path,
                    &envs,
                    format!("Running create hook: {hook_name}"),
                    self.mute,
                )
                .with_context(|| format!("failed to run create hook {hook_name:?}"))?;
            }
        }

        debug!("[op] Ensure repo create done");
        Ok(cloned)
    }

    pub fn remove(&self) -> Result<()> {
        if !self.path.exists() {
            return Ok(());
        }
        show_info!(self, "Remove dir {}", self.path.display());
        fs::remove_dir_all(&self.path).context("remove directory")?;

        let path = PathBuf::from(&self.path);
        let dir = path.parent();
        if dir.is_none() {
            return Ok(());
        }
        let mut dir = dir.unwrap();
        loop {
            match fs::read_dir(dir) {
                Ok(dir_read) => {
                    let count = dir_read.count();
                    if count > 0 {
                        return Ok(());
                    }
                    show_info!(self, "Remove dir {}", dir.display());
                    fs::remove_dir(dir).context("remove directory")?;
                    match dir.parent() {
                        Some(parent) => dir = parent,
                        None => return Ok(()),
                    }
                }
                Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
                Err(err) => {
                    return Err(err).with_context(|| format!("read dir '{}'", dir.display()));
                }
            }
        }
    }

    fn ensure_remote(&self) -> Result<()> {
        debug!("[op] Ensure repo remote");

        let Some(url) = self.get_clone_url() else {
            debug!("[op] Remote does not support clone, skip ensure_remote");
            return Ok(());
        };

        let Some(remote) = Remote::origin(Some(&self.path), self.mute)? else {
            debug!("[op] Repo has no origin remote, add: {url:?}");
            return self.git(
                ["remote", "add", "origin", &url],
                format!("Add origin remote: {url}"),
            );
        };

        let current_url = remote.get_url()?;
        if current_url == url {
            debug!("[op] Repo origin remote url is up-to-date: {url:?}");
            return Ok(());
        }

        debug!("[op] Repo origin remote url is different, current: {current_url:?}, new: {url:?}");
        self.git(
            ["remote", "set-url", "origin", &url],
            format!("Set origin remote: {url}"),
        )
    }

    fn ensure_user(&self) -> Result<()> {
        if let Some(ref user) = self.owner.user {
            debug!("[op] Set user.name to {user:?}");
            let message = format!("Set user to {user:?}");
            self.git(["config", "user.name", user], message)?;
        }

        if let Some(ref email) = self.owner.email {
            debug!("[op] Set user.email to {email:?}");
            let message = format!("Set email to {email:?}");
            self.git(["config", "user.email", email], message)?;
        }

        Ok(())
    }

    pub async fn get_git_remote<'c>(
        &'c self,
        upstream: bool,
        force_no_cache: bool,
    ) -> Result<Remote<'c>> {
        debug!("[op] Get git remote, upstream: {upstream}, force_no_cache: {force_no_cache}");
        if !upstream {
            let Some(remote) = Remote::origin(Some(&self.path), self.mute)? else {
                bail!("repository does not have origin remote, please sync first");
            };
            debug!("[op] Get origin remote: {remote:?}");
            return Ok(remote);
        }

        let remotes = Remote::list(Some(&self.path), false)?;
        for remote in remotes {
            if remote.name == "upstream" {
                debug!("[op] Get upstream remote: {remote:?}");
                return Ok(remote);
            }
        }

        debug!("[op] Upstream remote not found, try to add it");
        let Some(ref domain) = self.remote.clone else {
            bail!(
                "remote {:?} does not support clone, cannot get upstream",
                self.repo.remote
            );
        };

        let api = self.ctx.get_api(&self.repo.remote, force_no_cache)?;

        show_info!(self, "Get upstream info for repo {}", self.repo.full_name());
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

        if !self.mute {
            confirm!(
                "Do you want to set upstream to {}/{}: {upstream_url}",
                upstream.owner,
                upstream.name
            );
        }

        show_info!(self, "Set upstream remote to {upstream_url:?}");
        self.git(
            ["remote", "add", "upstream", &upstream_url],
            format!("Set upstream to {upstream_url}"),
        )?;

        let remote = Remote::new(String::from("upstream"), Some(&self.path), self.mute);
        debug!("[op] Add upstream remote: {remote:?}");
        Ok(remote)
    }

    pub fn sync(&self) -> Result<SyncResult> {
        debug!("[op] Begin to sync repo");
        let cloned = self.ensure_create(false, None)?;
        if !cloned {
            debug!("[op] Repo not cloned, ensure user and remote");
            self.ensure_user()?;
            self.ensure_remote()?;
        }

        let mut result = SyncResult {
            name: self.repo.full_name(),
            ..Default::default()
        };

        self.git(
            ["fetch", "origin", "--prune", "--prune-tags"],
            "Fetching origin remote",
        )?;

        let uncomitted = count_uncommitted_changes(Some(&self.path), self.mute)?;
        if uncomitted > 0 {
            debug!("[op] Repo has {uncomitted} uncommitted changes, skip sync branches");
            result.uncomitted = uncomitted;
            return Ok(result);
        }

        let branches = Branch::list(Some(&self.path), self.mute)?;
        let default_branch = Branch::default(Some(&self.path), self.mute)?;
        let mut back = default_branch.clone();
        debug!(
            "[op] Begin to sync branches, default_branch: {default_branch}, branches: {branches:?}"
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

        debug!("[op] Sync branch tasks: {tasks:?}, back: {back}, current: {current}");

        if tasks.is_empty() {
            debug!("[op] No branch to sync");
            show_info!(self, "No branch to sync");
            return Ok(result);
        }

        show_info!(self, "Backup branch is {}", style(&back).magenta().bold());

        for task in tasks {
            debug!("[op] Begin to handle sync branch task: {task:?}");
            match task.action {
                BranchAction::Push | BranchAction::Pull => {
                    if current != task.branch {
                        debug!("[op] Checkout to branch {} to push or pull", task.branch);
                        // checkout to this branch to perform push/pull
                        self.git(
                            ["checkout", &task.branch],
                            format!("Checkout to branch {}", task.branch),
                        )?;
                        current = task.branch.clone();
                    }

                    let (title, op) = match task.action {
                        BranchAction::Push => ("Pushing", "push"),
                        BranchAction::Pull => ("Pulling", "pull"),
                        _ => unreachable!(),
                    };
                    debug!("[op] {title} branch {}", task.branch);
                    self.git(
                        [op, "origin", &task.branch],
                        format!("{title} branch {}", task.branch),
                    )?;
                }
                BranchAction::Delete => {
                    if current == task.branch {
                        debug!("[op] Checkout to default branch {default_branch} before delete");
                        // we cannot delete branch when we are inside it, checkout
                        // to default branch first.
                        self.git(
                            ["checkout", &default_branch],
                            format!("Checkout to default branch {default_branch}"),
                        )?;
                    }

                    debug!("[op] Deleting branch {}", task.branch);
                    self.git(
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
            debug!("[op] Checkout to backup branch {back:?}");
            self.git(
                ["checkout", &back],
                format!("Checkout to backup branch {back}"),
            )?;
        }

        debug!("[op] Sync branches done, result: {result:?}");
        Ok(result)
    }

    pub async fn rebase(&self, opts: RebaseOptions<'_>) -> Result<()> {
        debug!("[op] Begin to rebase repo, options: {opts:?}");
        ensure_no_uncommitted_changes(Some(&self.path), self.mute)?;

        let remote = self
            .get_git_remote(opts.upstream, opts.force_no_cache)
            .await?;
        debug!("[op] Get remote for rebase: {remote:?}");

        let target = remote.get_target(opts.target)?;
        debug!("[op] Get target for rebase: {target:?}");

        self.git(["rebase", &target], format!("Rebasing from {target}"))
    }

    pub async fn squash(&self, opts: SquashOptions<'_>) -> Result<()> {
        debug!("[op] Begin to squash repo, options: {opts:?}");
        ensure_no_uncommitted_changes(Some(&self.path), self.mute)?;

        let remote = self
            .get_git_remote(opts.upstream, opts.force_no_cache)
            .await?;
        debug!("[op] Get remote for squash: {remote:?}");

        let commits = remote.commits_between(opts.target)?;
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

        if !self.mute {
            info!("Found {} commits to squash:", commits.len());
            for commit in commits.iter() {
                outputln!("  * {commit}");
            }
            confirm!("Continue");
        }

        debug!("[op] Soft reset to squash commits");
        let set = format!("HEAD~{}", commits.len());
        self.git(["reset", "--soft", &set], "Soft reset to squash")?;

        debug!("[op] Commit squashed changes");
        let args = if let Some(message) = opts.message {
            vec!["commit", "--message", message.as_str()]
        } else {
            vec!["commit"]
        };
        self.git(args, "Commit squashed changes")
    }

    #[inline]
    fn get_clone_url(&self) -> Option<String> {
        let domain = self.remote.clone.as_ref()?;
        Some(self.repo.get_clone_url(domain, self.owner.ssh))
    }

    #[inline]
    fn git<I, S>(&self, args: I, message: impl ToString) -> Result<()>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        git::new(args, Some(&self.path), message, self.mute).execute()
    }
}

impl SyncResult {
    pub fn render(&self, with_header: bool) -> String {
        let mut fields = vec![];
        if self.uncomitted > 0 {
            let flag = style("*").yellow().bold();
            let field = format!("  {flag} {} dirty", self.uncomitted);
            fields.push(field);
        }

        if !self.pushed.is_empty() {
            let flag = style("↑").green().bold();
            let field = format!("  {flag} {}", self.pushed.len());
            fields.push(field);
        }

        if !self.pulled.is_empty() {
            let flag = style("↓").green().bold();
            let field = format!("  {flag} {}", self.pulled.len());
            fields.push(field);
        }

        if !self.deleted.is_empty() {
            let flag = style("-").red().bold();
            let field = format!("  {flag} {}", self.deleted.len());
            fields.push(field);
        }

        if !self.conflect.is_empty() {
            let flag = style("$").red().bold();
            let field = format!("  {flag} {}", self.conflect.len());
            fields.push(field);
        }

        if !self.detached.is_empty() {
            let flag = style("?").red().bold();
            let field = format!("  {flag} {}", self.detached.len());
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
    use std::path::Path;

    use crate::config::context;
    use crate::repo::select::RepoSelector;

    use super::*;

    #[tokio::test]
    async fn test_create_clone() {
        if !git::tests::enable() {
            return;
        }

        let ctx = context::tests::build_test_context("create_clone", None);
        let args = ["roxide".to_string()];
        let selector = RepoSelector::from_args(ctx.clone(), &args);
        let repo = selector.select_one(false, true).await.unwrap();
        let path = repo.get_path(&ctx.cfg.workspace);
        let op = RepoOperator::new(ctx.as_ref(), &repo, true).unwrap();
        op.ensure_create(false, None).unwrap();

        let git_remote = git::remote::Remote::origin(Some(&path), true)
            .unwrap()
            .unwrap();
        assert_eq!(git_remote.name, "origin");

        let url = git_remote.get_url().unwrap();
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
        let ctx = context::tests::build_test_context("create_empty", None);
        let path = repo.get_path(&ctx.cfg.workspace);
        let op = RepoOperator::new(ctx.as_ref(), &repo, true).unwrap();
        op.ensure_create(true, None).unwrap();

        git::new(["status"], Some(&path), "", true)
            .execute()
            .unwrap();

        // The cargo-init hook should have created a Cargo.toml
        let cargo_path = path.join("Cargo.toml");
        fs::metadata(&cargo_path).unwrap();
    }

    #[test]
    fn test_remove() {
        let repo = Repository {
            remote: "test".to_string(),
            owner: "rust".to_string(),
            name: "hello".to_string(),
            ..Default::default()
        };
        let ctx = context::tests::build_test_context("remove", None);
        let path = repo.get_path(&ctx.cfg.workspace);
        let op = RepoOperator::new(ctx.as_ref(), &repo, true).unwrap();
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

        let ctx = context::tests::build_test_context("get_git_remote", None);
        let repo = Repository {
            remote: "github".to_string(),
            owner: "fioncat".to_string(),
            name: "example".to_string(),
            ..Default::default()
        };
        let op = RepoOperator::new(ctx.as_ref(), &repo, true).unwrap();
        op.ensure_create(true, None).unwrap();

        let path = repo.get_path(&ctx.cfg.workspace);
        assert!(path.exists());

        let remote = op.get_git_remote(false, false).await.unwrap();
        assert_eq!(remote.name, "origin");
    }

    #[tokio::test]
    async fn test_get_git_remote_upstream() {
        if !git::tests::enable() {
            return;
        }

        let ctx = context::tests::build_test_context("get_git_remote_upstream", None);
        let repo = Repository {
            remote: "github".to_string(),
            owner: "fioncat".to_string(),
            name: "nvimdots".to_string(),
            ..Default::default()
        };
        let op = RepoOperator::new(ctx.as_ref(), &repo, true).unwrap();
        op.ensure_create(true, None).unwrap();

        let path = repo.get_path(&ctx.cfg.workspace);
        assert!(path.exists());

        let remote = op.get_git_remote(true, true).await.unwrap();
        assert_eq!(remote.name, "upstream");

        let url = remote.get_url().unwrap();
        assert_eq!(url, "https://github.com/ayamir/nvimdots.git");
    }

    #[test]
    fn test_sync_branch() {
        if !git::tests::enable() {
            return;
        }

        let ctx = context::tests::build_test_context("sync_branch", None);
        let repo = Repository {
            remote: "github".to_string(),
            owner: "fioncat".to_string(),
            name: "example".to_string(),
            ..Default::default()
        };
        let op = RepoOperator::new(ctx.as_ref(), &repo, true).unwrap();
        op.ensure_create(false, None).unwrap();

        let path = repo.get_path(&ctx.cfg.workspace);
        assert!(path.exists());

        // Reset a commit, to test pulling
        op.git(["reset", "--hard", "HEAD~1"], "Reset last commit")
            .unwrap();

        // Create a new branch, to test pushing
        op.git(["checkout", "-b", "test-push"], "Create new branch")
            .unwrap();
        op.git(
            ["push", "origin", "--set-upstream", "test-push"],
            "Push new branch",
        )
        .unwrap();
        fs::write(path.join("test.txt"), "test").unwrap();
        op.git(["add", "."], "Add file").unwrap();
        op.git(["commit", "-m", "Add test file"], "Commit file")
            .unwrap();

        op.git(
            ["checkout", "-b", "test-detached"],
            "Create detached branch",
        )
        .unwrap();

        let branches = Branch::list(Some(&path), true).unwrap();
        let push_branch = branches.iter().find(|b| b.name == "test-push").unwrap();
        let pull_branch = branches.iter().find(|b| b.name == "master").unwrap();
        let deatched_branch = branches.iter().find(|b| b.name == "test-detached").unwrap();
        assert_eq!(push_branch.status, BranchStatus::Ahead);
        assert_eq!(pull_branch.status, BranchStatus::Behind);
        assert_eq!(deatched_branch.status, BranchStatus::Detached);

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

        let branches = Branch::list(Some(&path), true).unwrap();
        let push_branch = branches.iter().find(|b| b.name == "test-push").unwrap();
        let pull_branch = branches.iter().find(|b| b.name == "master").unwrap();
        let deatched_branch = branches.iter().find(|b| b.name == "test-detached").unwrap();
        assert_eq!(push_branch.status, BranchStatus::Sync);
        assert_eq!(pull_branch.status, BranchStatus::Sync);
        assert_eq!(deatched_branch.status, BranchStatus::Detached);

        let current = branches.iter().find(|b| b.current).unwrap();
        assert_eq!(current.name, "test-detached");

        // Cleanup
        op.git(["branch", "-D", "test-push"], "Delete test-push branch")
            .unwrap();
        op.git(
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

        let ctx = context::tests::build_test_context("sync_uncomitted", None);
        let repo = Repository {
            remote: "github".to_string(),
            owner: "fioncat".to_string(),
            name: "example".to_string(),
            ..Default::default()
        };
        let op = RepoOperator::new(ctx.as_ref(), &repo, true).unwrap();
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
                uncomitted: 3,
                ..Default::default()
            }
        );
    }

    #[tokio::test]
    async fn test_sync_create() {
        if !git::tests::enable() {
            return;
        }

        let ctx = context::tests::build_test_context("sync_create", None);
        let repo = Repository {
            remote: "github".to_string(),
            owner: "fioncat".to_string(),
            name: "example".to_string(),
            ..Default::default()
        };
        let op = RepoOperator::new(ctx.as_ref(), &repo, true).unwrap();

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
        assert_eq!(remote.name, "origin");
    }

    #[tokio::test]
    async fn test_sync_ensure() {
        if !git::tests::enable() {
            return;
        }
        let ctx = context::tests::build_test_context("sync_ensure", None);
        let repo = Repository {
            remote: "github".to_string(),
            owner: "fioncat".to_string(),
            name: "example".to_string(),
            ..Default::default()
        };
        let op = RepoOperator::new(ctx.as_ref(), &repo, true).unwrap();
        op.ensure_create(true, None).unwrap();

        let path = repo.get_path(&ctx.cfg.workspace);
        assert!(path.exists());

        op.git(["config", "user.name", "test-user"], "Set user name")
            .unwrap();
        op.git(
            ["config", "user.email", "test-email@test.com"],
            "Set user email",
        )
        .unwrap();
        op.git(
            [
                "remote",
                "set-url",
                "origin",
                "https://github.com/fioncat/roxide.git",
            ],
            "Update origin remote",
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

        let name = git::new(["config", "user.name"], Some(&path), "", true)
            .output()
            .unwrap();
        let email = git::new(["config", "user.email"], Some(&path), "", true)
            .output()
            .unwrap();
        assert_eq!(name, "fioncat");
        assert_eq!(email, "lazycat7706@gmail.com");

        let remote = op.get_git_remote(false, false).await.unwrap();
        assert_eq!(remote.name, "origin");
        let url = remote.get_url().unwrap();
        assert_eq!(url, op.get_clone_url().unwrap());
    }
}

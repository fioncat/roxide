use std::borrow::Cow;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Instant;

use anyhow::{bail, Context, Result};
use clap::Args;
use console::style;
use semver::VersionReq;

use crate::api::{self, Provider};
use crate::cmd::Run;
use crate::config::{Config, RemoteConfig};
use crate::repo::database::Database;
use crate::repo::Repo;
use crate::{confirm, term, utils};

/// Check system environment.
#[derive(Args)]
pub struct CheckArgs {}

impl Run for CheckArgs {
    fn run(&self, cfg: &Config) -> Result<()> {
        let mut db = Database::load(cfg)?;

        let mut checks: Vec<Box<dyn Check>> = vec![
            Box::new(CheckGit::new()),
            Box::new(CheckFzf::new()),
            Box::new(CheckDir::new()),
            Box::new(CheckShell::new()),
        ];

        let remote_names = cfg.list_remotes();
        for remote_name in remote_names {
            let remote_cfg = match cfg.get_remote(&remote_name) {
                Some(cfg) => cfg,
                None => continue,
            };
            if remote_cfg.provider.is_none() {
                continue;
            }

            let check_remote_api = CheckRemoteApi::new(remote_name, cfg, &remote_cfg)?;
            checks.push(Box::new(check_remote_api));
        }

        let mut to_remove = Vec::new();
        let mut ok_count = 0;
        let mut fail_count = 0;
        let start = Instant::now();
        Self::run_checks(
            checks,
            cfg,
            &db,
            &mut to_remove,
            false,
            &mut ok_count,
            &mut fail_count,
        );
        let elapsed_time = start.elapsed();
        let result = if fail_count > 0 {
            style("failed").red().to_string()
        } else {
            style("ok").green().to_string()
        };

        eprintln!();
        eprintln!(
            "Check result: {result}. {ok_count} ok; {fail_count} failed; finished in {}",
            utils::format_elapsed(elapsed_time)
        );

        if !to_remove.is_empty() {
            eprintln!();
            confirm!("Do you want to remove failed repos");

            for repo in to_remove {
                let path = repo.get_path(cfg);
                utils::remove_dir_recursively(path, true)?;
                db.remove(repo);
            }

            db.save()?;
        }

        Ok(())
    }
}

impl CheckArgs {
    fn run_checks(
        checks: Vec<Box<dyn Check>>,
        cfg: &Config,
        db: &Database,
        to_remove: &mut Vec<Repo>,
        is_sub: bool,
        ok_count: &mut usize,
        fail_count: &mut usize,
    ) {
        let hint_prefix = if is_sub { ":: " } else { "" };
        for check in checks {
            let name = check.name();

            eprintln!("{hint_prefix}Checking {name} ...");
            let result = check.check(cfg, db);
            term::cursor_up();
            eprint!("{hint_prefix}Check {name} ");

            match result {
                Ok(result) => {
                    *ok_count += 1;
                    eprint!("{}", style("✔").green().bold());
                    if let Some(hint) = result.hint.as_ref() {
                        eprintln!(" {}", style(hint).bold());
                    } else {
                        eprintln!(" ");
                    }
                    if let Some(subs) = result.subs {
                        Self::run_checks(subs, cfg, db, to_remove, true, ok_count, fail_count);
                    }
                }
                Err(err) => {
                    *fail_count += 1;
                    let msg = err.to_string();
                    eprintln!("{} {:#}", style("✘").red().bold(), style(msg).yellow());
                    if let Some(repo) = check.get_repo() {
                        to_remove.push(repo.update());
                    }
                }
            };
        }
    }
}

trait Check {
    fn name(&self) -> Cow<'static, str>;
    fn check(&self, cfg: &Config, db: &Database) -> Result<CheckResult>;
    fn get_repo(&self) -> Option<Repo>;
}

struct CheckResult {
    hint: Option<String>,
    subs: Option<Vec<Box<dyn Check>>>,
}

struct CheckGit {}

impl CheckGit {
    const REQ_GIT_VERSION: &'static str = ">=1.20.0";

    fn new() -> Self {
        Self {}
    }
}

impl Check for CheckGit {
    fn name(&self) -> Cow<'static, str> {
        Cow::Borrowed("git")
    }

    fn check(&self, _cfg: &Config, _db: &Database) -> Result<CheckResult> {
        let ver = term::git_version()?;
        let req = VersionReq::parse(Self::REQ_GIT_VERSION).unwrap();

        if !req.matches(&ver) {
            bail!("git version too low, require >= 1.20.0");
        }

        Ok(CheckResult {
            hint: Some(ver.to_string()),
            subs: None,
        })
    }

    fn get_repo(&self) -> Option<Repo> {
        None
    }
}

struct CheckFzf {}

impl CheckFzf {
    const REQ_FZF_VERSION: &'static str = ">=0.40.0";

    fn new() -> Self {
        Self {}
    }
}

impl Check for CheckFzf {
    fn name(&self) -> Cow<'static, str> {
        Cow::Borrowed("fzf")
    }

    fn check(&self, _cfg: &Config, _db: &Database) -> Result<CheckResult> {
        let ver = term::fzf_version()?;
        let req = VersionReq::parse(Self::REQ_FZF_VERSION).unwrap();

        if !req.matches(&ver) {
            bail!("fzf version too low, require >= 0.40.0");
        }

        Ok(CheckResult {
            hint: Some(ver.to_string()),
            subs: None,
        })
    }

    fn get_repo(&self) -> Option<Repo> {
        None
    }
}

struct CheckDir {}

impl CheckDir {
    fn new() -> Self {
        Self {}
    }

    fn check_dir(path: PathBuf) -> Result<()> {
        let test_file = path.join(".test_roxide_file");
        utils::ensure_dir(&test_file).context("ensure dir")?;

        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&test_file)
            .context("open test file")?;

        let test_data = &[1, 2, 3, 4, 5];
        file.write_all(test_data).context("write test file")?;
        drop(file);

        fs::remove_file(&test_file).context("remove test file")?;

        Ok(())
    }
}

impl Check for CheckDir {
    fn name(&self) -> Cow<'static, str> {
        Cow::Borrowed("dir")
    }

    fn check(&self, cfg: &Config, _db: &Database) -> Result<CheckResult> {
        let workspace = PathBuf::from(&cfg.get_workspace_dir());
        Self::check_dir(workspace).context("check workspace dir")?;

        let meta_dir = PathBuf::from(&cfg.get_meta_dir());
        Self::check_dir(meta_dir).context("check meta dir")?;

        Ok(CheckResult {
            hint: None,
            subs: None,
        })
    }

    fn get_repo(&self) -> Option<Repo> {
        None
    }
}

struct CheckShell {}

impl CheckShell {
    fn new() -> Self {
        Self {}
    }
}

impl Check for CheckShell {
    fn name(&self) -> Cow<'static, str> {
        Cow::Borrowed("shell")
    }

    fn check(&self, _cfg: &Config, _db: &Database) -> Result<CheckResult> {
        let shell = term::shell_type()?;
        match shell.as_str() {
            "bash" | "zsh" => Ok(CheckResult {
                hint: Some(shell),
                subs: None,
            }),
            _ => bail!("unsupported shell {shell}"),
        }
    }

    fn get_repo(&self) -> Option<Repo> {
        None
    }
}

struct CheckRemoteApi {
    name: String,
    provider: Rc<Box<dyn Provider>>,
}

impl CheckRemoteApi {
    fn new(name: String, cfg: &Config, remote_cfg: &RemoteConfig) -> Result<Self> {
        let provider = api::build_provider(cfg, remote_cfg, true)?;
        Ok(Self {
            name,
            provider: Rc::new(provider),
        })
    }
}

impl Check for CheckRemoteApi {
    fn name(&self) -> Cow<'static, str> {
        Cow::Owned(format!("{} remote api", self.name))
    }

    fn check(&self, _cfg: &Config, db: &Database) -> Result<CheckResult> {
        let info = self.provider.info().context("get api info")?;
        if !info.ping {
            bail!("remote api server is not available");
        }

        let repos = db.list_by_remote(self.name.as_str(), &None);
        let mut repo_checks: Vec<Box<dyn Check>> = Vec::with_capacity(repos.len());
        for repo in repos {
            let repo_check = CheckRepoApi {
                repo: repo.update(),
                provider: Rc::clone(&self.provider),
            };
            repo_checks.push(Box::new(repo_check));
        }

        let hint = if info.auth {
            format!("{}, with auth", info.name)
        } else {
            format!("{}, no auth", info.name)
        };

        Ok(CheckResult {
            hint: Some(hint),
            subs: Some(repo_checks),
        })
    }

    fn get_repo(&self) -> Option<Repo> {
        None
    }
}

struct CheckRepoApi<'a> {
    repo: Repo<'a>,
    provider: Rc<Box<dyn Provider>>,
}

impl Check for CheckRepoApi<'_> {
    fn name(&self) -> Cow<'static, str> {
        Cow::Owned(self.repo.name_with_owner())
    }

    fn check(&self, _cfg: &Config, _db: &Database) -> Result<CheckResult> {
        self.provider.get_repo(&self.repo.owner, &self.repo.name)?;
        Ok(CheckResult {
            hint: None,
            subs: None,
        })
    }

    fn get_repo(&self) -> Option<Repo> {
        Some(self.repo.clone())
    }
}

use std::time::Duration;

use anyhow::{bail, Context, Result};
use reqwest::blocking::{Client, Request};
use reqwest::{Method, Url};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::api::types::{ApiRepo, ApiUpstream, MergeOptions, Provider};
use crate::config::types::Remote;

#[derive(Debug, Deserialize)]
struct Repo {
    pub name: String,
    pub html_url: String,

    pub source: Option<Source>,

    pub default_branch: String,
}

#[derive(Debug, Deserialize)]
struct Source {
    pub name: String,
    pub owner: Owner,
    pub default_branch: String,
}

#[derive(Debug, Deserialize)]
struct Owner {
    pub login: String,
}

#[derive(Debug, Deserialize)]
struct Error {
    pub message: String,
}

#[derive(Debug, Deserialize)]
struct Release {
    pub tag_name: String,
}

impl Repo {
    fn to_api(self) -> ApiRepo {
        let Repo {
            name,
            html_url,
            source,
            default_branch,
        } = self;
        let upstream = match source {
            Some(source) => Some(ApiUpstream {
                owner: source.owner.login,
                name: source.name,
                default_branch: source.default_branch,
            }),
            None => None,
        };
        ApiRepo {
            name,
            default_branch,
            upstream,
            web_url: html_url,
        }
    }
}

struct PullRequestOptions {
    owner: String,
    name: String,

    head: String,
    head_search: String,

    base: String,
}

#[derive(Debug, Serialize)]
struct PullRequestBody {
    head: String,
    base: String,

    title: String,
    body: String,
}

impl From<MergeOptions> for PullRequestOptions {
    fn from(merge: MergeOptions) -> Self {
        let MergeOptions {
            mut owner,
            mut name,
            upstream,
            source,
            target,
        } = merge;

        let (head, head_search) = match upstream {
            Some(upstream) => {
                let head = format!("{owner}:{source}");
                let head_search = format!("{owner}/{source}");
                (owner, name) = (upstream.owner, upstream.name);
                (head, head_search)
            }
            None => {
                let head_search = source.clone();
                (source, head_search)
            }
        };
        PullRequestOptions {
            owner,
            name,
            head,
            head_search,
            base: target,
        }
    }
}

#[derive(Debug, Deserialize)]
struct PullRequest {
    html_url: String,
}

pub struct Github {
    token: Option<String>,

    client: Client,

    per_page: u32,
}

impl Provider for Github {
    fn list_repos(&self, owner: &str) -> Result<Vec<String>> {
        let path = format!("users/{owner}/repos?per_page={}", self.per_page);
        let github_repos = self.execute_get::<Vec<Repo>>(&path)?;
        let repos: Vec<String> = github_repos.into_iter().map(|repo| repo.name).collect();
        Ok(repos)
    }

    fn get_repo(&self, owner: &str, name: &str) -> Result<ApiRepo> {
        let path = format!("repos/{}/{}", owner, name);
        Ok(self.execute_get::<Repo>(&path)?.to_api())
    }

    fn get_merge(&self, merge: MergeOptions) -> Result<Option<String>> {
        let opts: PullRequestOptions = merge.into();
        let head = urlencoding::encode(&opts.head_search);
        let base = urlencoding::encode(&opts.base);
        let path = format!(
            "repos/{}/{}/pulls?state=open&head={}&base={}",
            opts.owner, opts.name, head, base
        );
        let mut prs = self.execute_get::<Vec<PullRequest>>(&path)?;
        if prs.is_empty() {
            return Ok(None);
        }
        Ok(Some(prs.remove(0).html_url))
    }

    fn create_merge(&self, merge: MergeOptions, title: String, body: String) -> Result<String> {
        let opts: PullRequestOptions = merge.into();
        let path = format!("repos/{}/{}/pulls", opts.owner, opts.name);
        let body = PullRequestBody {
            head: opts.head,
            base: opts.base,
            title,
            body,
        };
        let pr = self.execute_post::<PullRequestBody, PullRequest>(&path, body)?;
        Ok(pr.html_url)
    }
}

impl Github {
    const API_VERSION: &str = "2022-11-28";

    pub fn new(remote: &Remote) -> Box<dyn Provider> {
        let client = super::build_common_client(remote);
        Box::new(Github {
            token: remote.token.clone(),
            per_page: remote.list_limit,
            client,
        })
    }

    pub fn new_empty() -> Github {
        let client = Client::builder()
            .timeout(Duration::from_secs_f32(20.0))
            .build()
            .unwrap();
        Github {
            token: None,
            per_page: 30,
            client,
        }
    }

    fn execute_get<T>(&self, path: &str) -> Result<T>
    where
        T: DeserializeOwned + ?Sized,
    {
        let req = self.build_request(path, Method::GET, None)?;
        self.execute(req)
    }

    fn execute_post<B, R>(&self, path: &str, body: B) -> Result<R>
    where
        B: Serialize,
        R: DeserializeOwned + ?Sized,
    {
        let body = serde_json::to_vec(&body).context("Encode Github request body")?;
        let req = self.build_request(path, Method::POST, Some(body))?;
        self.execute(req)
    }

    fn execute<T>(&self, req: Request) -> Result<T>
    where
        T: DeserializeOwned + ?Sized,
    {
        let resp = self.client.execute(req).context("Github http request")?;
        let ok = resp.status().is_success();
        let data = resp.bytes().context("Read Github response body")?;
        if ok {
            return serde_json::from_slice(&data).context("Decode Github response data");
        }

        match serde_json::from_slice::<Error>(&data) {
            Ok(err) => bail!("Github api error: {}", err.message),
            Err(_err) => bail!(
                "Unknown Github api error: {}",
                String::from_utf8(data.to_vec())
                    .context("Decode Github response to UTF-8 string")?
            ),
        }
    }

    fn build_request(&self, path: &str, method: Method, body: Option<Vec<u8>>) -> Result<Request> {
        let url = format!("https://api.github.com/{path}");
        let url = Url::parse(url.as_str()).with_context(|| format!("Parse url {url}"))?;
        let mut builder = self.client.request(method, url);
        builder = builder
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "roxide-client")
            .header("X-GitHub-Api-Version", Self::API_VERSION);
        if let Some(token) = &self.token {
            let token_value = format!("Bearer {token}");
            builder = builder.header("Authorization", token_value);
        }
        if let Some(body) = body {
            builder = builder.body(body);
        }
        builder.build().context("Build Github request")
    }

    pub fn get_latest_tag(&self, owner: &str, name: &str) -> Result<String> {
        let path = format!("repos/{owner}/{name}/releases/latest");
        let release = self.execute_get::<Release>(&path)?;
        Ok(release.tag_name)
    }
}

use crate::config::SourcesConfig;
use crate::types::{CheckRun, CheckStatus, Issue, PR, Repo, ReviewStatus, Source, Visibility};
use anyhow::{Context, Result, bail};
use log::debug;
use serde::Deserialize;
use tokio::process::Command;

#[derive(Deserialize)]
struct RepoRaw {
    name: String,
    language: Option<String>,
    pushed_at: Option<String>,
    owner_login: String,
    #[serde(default)]
    stargazers_count: u32,
    #[serde(default)]
    forks_count: u32,
    #[serde(default)]
    open_issues_count: u32,
    #[serde(default)]
    visibility: Visibility,
    #[serde(default = "bool_true")]
    has_issues: bool,
    #[serde(default)]
    archived: bool,
}

fn bool_true() -> bool {
    true
}

/// Run a `gh` command, return stdout as String, bail on non-zero exit.
async fn gh_run(args: &[&str]) -> Result<String> {
    let out = Command::new("gh")
        .args(args)
        .output()
        .await
        .context("failed to run gh")?;
    if !out.status.success() {
        bail!("{}", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

pub async fn fetch_user() -> Result<String> {
    debug!("fetch_user");
    let stdout = gh_run(&["api", "user", "--jq", ".login"])
        .await
        .map_err(|e| {
            debug!("fetch_user error: {e}");
            e
        })?;
    let login = stdout.trim().to_string();
    debug!("fetch_user -> {login}");
    Ok(login)
}

async fn fetch_orgs() -> Result<Vec<String>> {
    let mut all_orgs: Vec<String> = Vec::new();
    let mut page = 1u32;
    loop {
        let endpoint = format!("user/memberships/orgs?per_page=100&page={page}");
        let stdout = gh_run(&["api", &endpoint, "--jq", ".[] | .organization.login"]).await?;
        let page_orgs: Vec<String> = stdout
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let count = page_orgs.len();
        all_orgs.extend(page_orgs);
        if count < 100 {
            break;
        }
        page += 1;
    }
    all_orgs.sort();
    Ok(all_orgs)
}

pub async fn fetch_sources(cfg: &SourcesConfig) -> Result<(Vec<Source>, String)> {
    let mut sources: Vec<Source> = Vec::with_capacity(1 + cfg.orgs.len() + cfg.users.len());
    let mut current_user = String::new();

    if cfg.include_self || cfg.auto_fetch_orgs {
        match fetch_user().await {
            Ok(login) if !login.is_empty() => {
                if cfg.include_self {
                    current_user.clone_from(&login);
                    sources.push(Source::User(login));
                } else {
                    current_user = login;
                }
            }
            _ => {} // not authenticated or API unavailable — skip
        }
    }

    if cfg.auto_fetch_orgs
        && let Ok(orgs) = fetch_orgs().await
    {
        for org in orgs {
            if !sources.iter().any(|s| s.owner() == org) {
                sources.push(Source::Org(org));
            }
        }
    }

    for org in &cfg.orgs {
        let org = org.trim();
        if !org.is_empty() && !sources.iter().any(|s| s.owner() == org) {
            sources.push(Source::Org(org.to_string()));
        }
    }

    for user in &cfg.users {
        let user = user.trim();
        if !user.is_empty() && !sources.iter().any(|s| s.owner() == user) {
            sources.push(Source::User(user.to_string()));
        }
    }

    Ok((sources, current_user))
}

pub async fn fetch_repos(
    source: &Source,
    current_user: &str,
    per_page: u32,
    page: u32,
) -> Result<Vec<Repo>> {
    debug!(
        "fetch_repos: {} per_page={per_page} page={page}",
        source.owner()
    );
    let owner = source.owner().to_string();
    let base = match source {
        Source::User(name) if name == current_user => "user/repos".to_string(),
        Source::User(name) => format!("users/{name}/repos"),
        Source::Org(name) => format!("orgs/{name}/repos"),
    };
    let per_page = per_page.clamp(1, 100);
    let endpoint = format!("{base}?per_page={per_page}&page={page}");
    let jq = ".[] | {name, language, pushed_at, owner_login: .owner.login, stargazers_count, forks_count, open_issues_count, visibility, has_issues, archived}";
    let raw = gh_run(&["api", &endpoint, "--jq", jq]).await?;
    let repos: Vec<Repo> = raw
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<RepoRaw>(l).ok())
        .filter(|r| r.owner_login == owner)
        .map(|r| Repo {
            name: r.name,
            language: r.language,
            pushed_at: r.pushed_at,
            stars: r.stargazers_count,
            forks: r.forks_count,
            issues: r.open_issues_count,
            visibility: r.visibility,
            has_issues: r.has_issues,
            archived: r.archived,
        })
        .collect();
    debug!("fetch_repos: {} -> {} repos", source.owner(), repos.len());
    Ok(repos)
}

pub async fn fetch_prs(org: &str, repo: &str, per_page: u32, page: u32) -> Result<Vec<PR>> {
    debug!("fetch_prs: {org}/{repo} per_page={per_page} page={page}");
    let per_page = per_page.clamp(1, 100);
    let endpoint = format!(
        "repos/{org}/{repo}/pulls?state=open&per_page={per_page}&page={page}&sort=created&direction=desc"
    );
    let jq = r#".[] | {number, title, login: .user.login, draft, state, created_at, updated_at, url: .html_url, requested_reviewers: ([.requested_reviewers[] | .login] + [.requested_teams[] | .slug]), labels: [.labels[].name], head_ref: .head.ref, base_ref: .base.ref, head_sha: .head.sha}"#;
    let raw = gh_run(&["api", &endpoint, "--jq", jq]).await?;
    let mut prs = Vec::new();
    let mut first_err: Option<String> = None;
    for line in raw.lines().filter(|l| !l.trim().is_empty()) {
        match serde_json::from_str::<PR>(line) {
            Ok(pr) => prs.push(pr),
            Err(e) if first_err.is_none() => {
                first_err = Some(format!("parse error: {e}\nraw: {line}"));
            }
            _ => {}
        }
    }
    if prs.is_empty()
        && let Some(err) = first_err
    {
        bail!("{err}");
    }
    debug!("fetch_prs: {org}/{repo} page={page} -> {} prs", prs.len());
    Ok(prs)
}

pub async fn fetch_review_status(owner: &str, repo: &str, pr_number: u64) -> ReviewStatus {
    debug!("fetch_review_status: {owner}/{repo}#{pr_number}");
    let endpoint = format!("repos/{owner}/{repo}/pulls/{pr_number}/reviews?per_page=100");
    let Ok(out) = Command::new("gh")
        .args(["api", &endpoint, "--jq", ".[] | .state"])
        .output()
        .await
    else {
        return ReviewStatus::Unknown;
    };
    if !out.status.success() {
        debug!(
            "fetch_review_status error: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
        return ReviewStatus::Unknown;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut approved = false;
    for state in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if state == "CHANGES_REQUESTED" {
            debug!("fetch_review_status: #{pr_number} -> ChangesRequested (early)");
            return ReviewStatus::ChangesRequested;
        }
        if state == "APPROVED" {
            approved = true;
        }
    }
    let result = if approved {
        ReviewStatus::Approved
    } else {
        ReviewStatus::Pending
    };
    debug!("fetch_review_status: #{pr_number} -> {result:?}");
    result
}

pub async fn fetch_check_runs(owner: &str, repo: &str, sha: &str) -> Vec<CheckRun> {
    if sha.is_empty() {
        return Vec::new();
    }
    let endpoint = format!("repos/{owner}/{repo}/commits/{sha}/check-runs");
    let jq = r#"[.check_runs[] | {id: .id, name: .name, url: .html_url, s: (if .conclusion == "failure" or .conclusion == "cancelled" or .conclusion == "timed_out" or .conclusion == "action_required" then "failing" elif .status == "in_progress" or .status == "queued" then "pending" elif .conclusion == "success" or .conclusion == "neutral" or .conclusion == "skipped" then "passing" else "unknown" end)}]"#;
    let Ok(out) = Command::new("gh")
        .args(["api", &endpoint, "--jq", jq])
        .output()
        .await
    else {
        return Vec::new();
    };
    if !out.status.success() {
        return Vec::new();
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let Ok(arr) = serde_json::from_str::<serde_json::Value>(text.trim()) else {
        return Vec::new();
    };
    let Some(items) = arr.as_array() else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let id = item["id"].as_u64().unwrap_or(0);
            let name = item["name"].as_str()?.to_string();
            let url = item["url"].as_str().unwrap_or("").to_string();
            let status = match item["s"].as_str()? {
                "passing" => CheckStatus::Passing,
                "failing" => CheckStatus::Failing,
                "pending" => CheckStatus::Pending,
                _ => CheckStatus::Unknown,
            };
            Some(CheckRun {
                id,
                name,
                url,
                status,
            })
        })
        .collect()
}

pub async fn rerun_check(owner: &str, repo: &str, check_run_id: u64) -> Result<()> {
    let endpoint = format!("repos/{owner}/{repo}/check-runs/{check_run_id}/rerequest");
    gh_run(&["api", "-X", "POST", &endpoint]).await?;
    Ok(())
}

pub async fn fetch_rate_limit() -> Result<(u32, u32)> {
    let text = gh_run(&[
        "api",
        "rate_limit",
        "--jq",
        r#".resources.core | "\(.remaining)/\(.limit)""#,
    ])
    .await?;
    let text = text.trim().to_string();
    let (rem, lim) = text
        .split_once('/')
        .context(format!("unexpected rate limit format: {text}"))?;
    let remaining = rem.parse::<u32>().context("failed to parse remaining")?;
    let limit = lim.parse::<u32>().context("failed to parse limit")?;
    Ok((remaining, limit))
}

pub async fn fetch_diff(org: &str, repo: &str, pr: u64) -> Result<String> {
    let out = Command::new("gh")
        .args([
            "pr",
            "diff",
            &pr.to_string(),
            "-R",
            &format!("{org}/{repo}"),
        ])
        .env("GH_PAGER", "")
        .env("NO_COLOR", "1")
        .output()
        .await
        .context("failed to run gh")?;
    if !out.status.success() {
        bail!("{}", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

pub async fn fetch_repo_frontpage(owner: &str, repo: &str) -> Result<(String, String)> {
    let desc_endpoint = format!("repos/{owner}/{repo}");
    let description = gh_run(&["api", &desc_endpoint, "--jq", ".description // \"\""])
        .await
        .map(|s| s.trim().to_string())
        .unwrap_or_default();

    let readme_endpoint = format!("repos/{owner}/{repo}/readme");
    let readme = gh_run(&[
        "api",
        &readme_endpoint,
        "--jq",
        r#".content | gsub("\n";"") | @base64d"#,
    ])
    .await
    .unwrap_or_default();

    Ok((description, readme))
}

pub async fn fetch_issues(
    owner: &str,
    repo: &str,
    per_page: u32,
    page: u32,
) -> Result<(Vec<Issue>, bool)> {
    debug!("fetch_issues: {owner}/{repo} per_page={per_page} page={page}");
    let per_page = per_page.clamp(1, 100);
    let endpoint =
        format!("repos/{owner}/{repo}/issues?state=open&per_page={per_page}&page={page}");
    // Include is_pr so we can compute has_more from the raw count before filtering
    let jq = r#".[] | {number, title, author: .user.login, state, created_at, labels: [.labels[].name], url: .html_url, is_pr: (.pull_request != null)}"#;
    let raw = gh_run(&["api", &endpoint, "--jq", jq]).await?;

    #[derive(serde::Deserialize)]
    struct Row {
        number: u64,
        title: String,
        author: String,
        state: String,
        created_at: String,
        labels: Vec<String>,
        url: String,
        is_pr: bool,
    }

    let rows: Vec<Row> = raw
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    let has_more = rows.len() == per_page as usize;
    let issues = rows
        .into_iter()
        .filter(|r| !r.is_pr)
        .map(|r| Issue {
            number: r.number,
            title: r.title,
            author: r.author,
            state: r.state,
            created_at: r.created_at,
            labels: r.labels,
            url: r.url,
        })
        .collect();
    Ok((issues, has_more))
}

pub async fn fetch_issue_body(owner: &str, repo: &str, number: u64) -> Result<String> {
    let endpoint = format!("repos/{owner}/{repo}/issues/{number}");
    let text = gh_run(&["api", &endpoint, "--jq", r#".body // """#]).await?;
    Ok(text.trim().to_string())
}

pub async fn fetch_pr_body(
    owner: &str,
    repo: &str,
    pr_number: u64,
) -> Result<(String, crate::types::MergeableState, u32, u32)> {
    debug!("fetch_pr_body: {owner}/{repo}#{pr_number}");
    let endpoint = format!("repos/{owner}/{repo}/pulls/{pr_number}");
    let raw = gh_run(&["api", &endpoint, "--jq", r#"{body: (.body // ""), mergeable_state: (.mergeable_state // "unknown"), additions: (.additions // 0), deletions: (.deletions // 0)}"#]).await?;
    #[derive(serde::Deserialize)]
    struct Resp {
        body: String,
        mergeable_state: crate::types::MergeableState,
        additions: u32,
        deletions: u32,
    }
    let resp: Resp = serde_json::from_str(&raw).context("parse pr body response")?;
    Ok((
        resp.body,
        resp.mergeable_state,
        resp.additions,
        resp.deletions,
    ))
}

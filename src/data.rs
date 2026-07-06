use crate::config::SourcesConfig;
use crate::types::{
    CheckRun, CheckStatus, Issue, PR, Repo, RepoId, RepoSortKey, ReviewStatus, Source, Visibility,
};
use anyhow::{Context, Result, bail};
use log::debug;
use serde::Deserialize;

#[derive(Deserialize)]
struct RepoRaw {
    name: String,
    language: Option<String>,
    pushed_at: Option<String>,
    created_at: Option<String>,
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
    #[serde(default = "bool_true")]
    has_pull_requests: bool,
    #[serde(default)]
    archived: bool,
    allow_auto_merge: Option<bool>,
}

fn bool_true() -> bool {
    true
}

#[allow(async_fn_in_trait)]
pub trait GhRunner {
    async fn run(&self, args: &[&str]) -> anyhow::Result<String>;
}

pub struct GhCli;

impl GhRunner for GhCli {
    async fn run(&self, args: &[&str]) -> anyhow::Result<String> {
        debug!("gh {}", args.join(" "));
        let out = tokio::process::Command::new("gh")
            .args(args)
            .env("GH_PAGER", "")
            .env("NO_COLOR", "1")
            .output()
            .await
            .context("failed to run gh")?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            debug!("gh {} error: {}", args.join(" "), stderr.trim());
            bail!("{}", stderr.trim());
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }
}

async fn gh_run(args: &[&str]) -> Result<String> {
    GhCli.run(args).await
}

pub async fn fetch_user() -> Result<String> {
    fetch_user_with(&GhCli).await
}

pub async fn fetch_user_with<R: GhRunner>(runner: &R) -> Result<String> {
    debug!("fetch_user");
    let stdout = runner
        .run(&["api", "user", "--jq", ".login"])
        .await
        .map_err(|e| {
            debug!("fetch_user error: {e}");
            e
        })?;
    let login = stdout.trim().to_string();
    debug!("fetch_user -> {login}");
    Ok(login)
}

async fn fetch_orgs_with<R: GhRunner>(runner: &R) -> Result<Vec<String>> {
    let mut all_orgs: Vec<String> = Vec::new();
    let mut page = 1u32;
    loop {
        let endpoint = format!("user/memberships/orgs?per_page=100&page={page}");
        let stdout = runner
            .run(&["api", &endpoint, "--jq", ".[] | .organization.login"])
            .await?;
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
    fetch_sources_with(&GhCli, cfg).await
}

pub async fn fetch_sources_with<R: GhRunner>(
    runner: &R,
    cfg: &SourcesConfig,
) -> Result<(Vec<Source>, String)> {
    let mut sources: Vec<Source> = Vec::with_capacity(1 + cfg.orgs.len() + cfg.users.len());
    let mut current_user = String::new();

    if cfg.include_self || cfg.auto_fetch_orgs {
        match fetch_user_with(runner).await {
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
        && let Ok(orgs) = fetch_orgs_with(runner).await
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
    sort_key: RepoSortKey,
) -> Result<Vec<Repo>> {
    fetch_repos_with(&GhCli, source, current_user, per_page, page, sort_key).await
}

pub async fn fetch_repos_with<R: GhRunner>(
    runner: &R,
    source: &Source,
    current_user: &str,
    per_page: u32,
    page: u32,
    sort_key: RepoSortKey,
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
    let (sort, direction) = sort_key.api_params();
    let endpoint =
        format!("{base}?per_page={per_page}&page={page}&sort={sort}&direction={direction}");
    let jq = ".[] | {name, language, pushed_at, created_at, owner_login: .owner.login, stargazers_count, forks_count, open_issues_count, visibility, has_issues, has_pull_requests, archived, allow_auto_merge}";
    let raw = runner.run(&["api", &endpoint, "--jq", jq]).await?;
    let repos: Vec<Repo> = raw
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<RepoRaw>(l).ok())
        .filter(|r| r.owner_login == owner)
        .map(|r| Repo {
            name: r.name,
            language: r.language,
            pushed_at: r.pushed_at,
            created_at: r.created_at,
            stars: r.stargazers_count,
            forks: r.forks_count,
            issues: r.open_issues_count,
            visibility: r.visibility,
            has_issues: r.has_issues,
            has_pull_requests: r.has_pull_requests,
            archived: r.archived,
            allow_auto_merge: r.allow_auto_merge.unwrap_or(false),
        })
        .collect();
    debug!("fetch_repos: {} -> {} repos", source.owner(), repos.len());
    Ok(repos)
}

pub async fn fetch_prs(repo: &RepoId, per_page: u32, page: u32) -> Result<Vec<PR>> {
    fetch_prs_with(&GhCli, repo, per_page, page).await
}

pub async fn fetch_prs_with<R: GhRunner>(
    runner: &R,
    repo: &RepoId,
    per_page: u32,
    page: u32,
) -> Result<Vec<PR>> {
    debug!("fetch_prs: {repo} per_page={per_page} page={page}");
    let per_page = per_page.clamp(1, 100);
    let endpoint = format!(
        "{}/pulls?state=open&per_page={per_page}&page={page}&sort=created&direction=desc",
        repo.api_base()
    );
    let jq = r#".[] | {number, title, author: (.user.login // "ghost"), draft, state, created_at, updated_at, url: .html_url, requested_reviewers: ([.requested_reviewers[] | .login] + [.requested_teams[] | .slug]), labels: [.labels[] | {name: .name, color: (.color // "8b949e")}], head_ref: .head.ref, base_ref: .base.ref, head_sha: .head.sha, comments: ((.comments // 0) + (.review_comments // 0))}"#;
    let raw = runner.run(&["api", &endpoint, "--jq", jq]).await?;
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
    debug!("fetch_prs: {repo} page={page} -> {} prs", prs.len());
    Ok(prs)
}

pub async fn fetch_source_prs(
    owner: &str,
    is_org: bool,
    per_page: u32,
    page: u32,
) -> Result<Vec<PR>> {
    fetch_source_prs_with(&GhCli, owner, is_org, per_page, page).await
}

pub async fn fetch_source_prs_with<R: GhRunner>(
    runner: &R,
    owner: &str,
    is_org: bool,
    per_page: u32,
    page: u32,
) -> Result<Vec<PR>> {
    debug!("fetch_source_prs: {owner} is_org={is_org} per_page={per_page} page={page}");
    let per_page = per_page.clamp(1, 100);
    let scope = if is_org {
        format!("org:{owner}")
    } else {
        format!("author:{owner}")
    };
    let endpoint = format!(
        "search/issues?q=is:pr+is:open+{scope}&sort=created&order=desc&per_page={per_page}&page={page}"
    );
    let jq = r#".items[] | {number, title, author: (.user.login // "ghost"), state, created_at, updated_at, url: .html_url, labels: [.labels[] | {name: .name, color: (.color // "8b949e")}], comments: ((.comments // 0)), repo: (.repository_url | split("/") | .[-1]), repo_owner: (.repository_url | split("/") | .[-2])}"#;
    let raw = runner.run(&["api", &endpoint, "--jq", jq]).await?;
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
    debug!("fetch_source_prs: {owner} page={page} -> {} prs", prs.len());
    Ok(prs)
}

pub async fn fetch_review_status(repo: &RepoId, pr_number: u64) -> ReviewStatus {
    fetch_review_status_with(&GhCli, repo, pr_number).await
}

pub async fn fetch_review_status_with<R: GhRunner>(
    runner: &R,
    repo: &RepoId,
    pr_number: u64,
) -> ReviewStatus {
    debug!("fetch_review_status: {repo}#{pr_number}");
    let endpoint = format!("{}/pulls/{pr_number}/reviews?per_page=100", repo.api_base());
    debug!("gh api {} --jq .[] | .state", endpoint);
    let Ok(text) = runner
        .run(&["api", &endpoint, "--jq", ".[] | .state"])
        .await
    else {
        return ReviewStatus::Unknown;
    };
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

pub async fn fetch_check_runs(repo: &RepoId, sha: &str) -> Vec<CheckRun> {
    fetch_check_runs_with(&GhCli, repo, sha).await
}

pub async fn fetch_check_runs_with<R: GhRunner>(
    runner: &R,
    repo: &RepoId,
    sha: &str,
) -> Vec<CheckRun> {
    if sha.is_empty() {
        return Vec::new();
    }
    let base = repo.api_base();
    let runs_endpoint = format!("{base}/commits/{sha}/check-runs");
    let runs_jq = r#"[.check_runs[] | {id: .id, name: .name, url: .html_url, suite_id: .check_suite.id, s: (if .conclusion == "failure" or .conclusion == "cancelled" or .conclusion == "timed_out" or .conclusion == "action_required" then "failing" elif .status == "in_progress" or .status == "queued" then "pending" elif .conclusion == "success" or .conclusion == "neutral" or .conclusion == "skipped" then "passing" else "unknown" end)}]"#;
    let workflows_endpoint = format!("{base}/actions/runs?head_sha={sha}");
    let workflows_jq = r"[.workflow_runs[] | {name, event, suite_id: .check_suite_id}]";

    debug!("gh api {runs_endpoint}");
    debug!("gh api {workflows_endpoint}");

    let runs_args = ["api", &runs_endpoint, "--jq", runs_jq];
    let wf_args = ["api", &workflows_endpoint, "--jq", workflows_jq];
    let (runs_out, wf_out) = tokio::join!(runner.run(&runs_args), runner.run(&wf_args));

    // Build suite_id → (workflow_name, event) map from workflow runs.
    let mut suite_map: std::collections::HashMap<u64, (String, String)> =
        std::collections::HashMap::new();
    if let Ok(text) = wf_out
        && let Ok(serde_json::Value::Array(arr)) =
            serde_json::from_str::<serde_json::Value>(text.trim())
    {
        for item in &arr {
            if let (Some(suite_id), Some(wf_name), Some(event)) = (
                item["suite_id"].as_u64(),
                item["name"].as_str(),
                item["event"].as_str(),
            ) {
                suite_map
                    .entry(suite_id)
                    .or_insert_with(|| (wf_name.to_string(), event.to_string()));
            }
        }
    }

    let Ok(text) = runs_out else {
        debug!("gh api {runs_endpoint} error: spawn failed");
        return Vec::new();
    };
    let Ok(serde_json::Value::Array(items)) =
        serde_json::from_str::<serde_json::Value>(text.trim())
    else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let id = item["id"].as_u64().unwrap_or(0);
            let raw_name = item["name"].as_str()?.to_string();
            let url = item["url"].as_str().unwrap_or("").to_string();
            let status = match item["s"].as_str()? {
                "passing" => CheckStatus::Passing,
                "failing" => CheckStatus::Failing,
                "pending" => CheckStatus::Pending,
                _ => CheckStatus::Unknown,
            };
            let name = if let Some(suite_id) = item["suite_id"].as_u64() {
                if let Some((wf_name, event)) = suite_map.get(&suite_id) {
                    format!("{wf_name} / {raw_name} ({event})")
                } else {
                    raw_name
                }
            } else {
                raw_name
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

pub async fn rerun_check(repo: &RepoId, check_run_id: u64) -> Result<()> {
    let endpoint = format!("{}/check-runs/{check_run_id}/rerequest", repo.api_base());
    gh_run(&["api", "-X", "POST", &endpoint]).await?;
    Ok(())
}

pub async fn fetch_rate_limit() -> Result<(u32, u32)> {
    fetch_rate_limit_with(&GhCli).await
}

pub async fn fetch_rate_limit_with<R: GhRunner>(runner: &R) -> Result<(u32, u32)> {
    let text = runner
        .run(&[
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

pub async fn fetch_diff(repo: &RepoId, pr: u64) -> Result<String> {
    fetch_diff_with(&GhCli, repo, pr).await
}

pub async fn fetch_diff_with<R: GhRunner>(runner: &R, repo: &RepoId, pr: u64) -> Result<String> {
    debug!("gh pr diff {pr} -R {repo}");
    let pr_s = pr.to_string();
    let repo_s = repo.to_string();
    runner.run(&["pr", "diff", &pr_s, "-R", &repo_s]).await
}

pub async fn fetch_repo_frontpage(repo: &RepoId) -> Result<(String, String)> {
    fetch_repo_frontpage_with(&GhCli, repo).await
}

pub async fn fetch_repo_frontpage_with<R: GhRunner>(
    runner: &R,
    repo: &RepoId,
) -> Result<(String, String)> {
    let base = repo.api_base();
    let description = runner
        .run(&["api", &base, "--jq", ".description // \"\""])
        .await
        .map(|s| s.trim().to_string())
        .unwrap_or_default();

    let readme_endpoint = format!("{base}/readme");
    let readme = runner
        .run(&[
            "api",
            &readme_endpoint,
            "--jq",
            r#".content | gsub("\n";"") | @base64d"#,
        ])
        .await
        .unwrap_or_default();

    Ok((description, readme))
}

pub async fn fetch_issues(repo: &RepoId, per_page: u32, page: u32) -> Result<(Vec<Issue>, bool)> {
    fetch_issues_with(&GhCli, repo, per_page, page).await
}

pub async fn fetch_issues_with<R: GhRunner>(
    runner: &R,
    repo: &RepoId,
    per_page: u32,
    page: u32,
) -> Result<(Vec<Issue>, bool)> {
    #[derive(serde::Deserialize)]
    struct Row {
        number: u64,
        title: String,
        author: String,
        created_at: String,
        labels: Vec<crate::types::Label>,
        url: String,
        is_pr: bool,
    }

    debug!("fetch_issues: {repo} per_page={per_page} page={page}");
    let per_page = per_page.clamp(1, 100);
    let endpoint = format!(
        "{}/issues?state=open&per_page={per_page}&page={page}",
        repo.api_base()
    );
    // Include is_pr so we can compute has_more from the raw count before filtering
    let jq = r#".[] | {number, title, author: (.user.login // "ghost"), created_at, labels: [.labels[] | {name: .name, color: (.color // "8b949e")}], url: .html_url, is_pr: (.pull_request != null)}"#;
    let raw = runner.run(&["api", &endpoint, "--jq", jq]).await?;
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
            created_at: r.created_at,
            labels: r.labels,
            url: r.url,
            repo: String::new(),
            repo_owner: String::new(),
        })
        .collect();
    Ok((issues, has_more))
}

pub async fn fetch_source_issues(
    owner: &str,
    is_org: bool,
    per_page: u32,
    page: u32,
) -> Result<Vec<Issue>> {
    fetch_source_issues_with(&GhCli, owner, is_org, per_page, page).await
}

pub async fn fetch_source_issues_with<R: GhRunner>(
    runner: &R,
    owner: &str,
    is_org: bool,
    per_page: u32,
    page: u32,
) -> Result<Vec<Issue>> {
    debug!("fetch_source_issues: {owner} is_org={is_org} per_page={per_page} page={page}");
    let per_page = per_page.clamp(1, 100);
    let scope = if is_org {
        format!("org:{owner}")
    } else {
        format!("author:{owner}")
    };
    let endpoint = format!(
        "search/issues?q=is:issue+is:open+{scope}&sort=created&order=desc&per_page={per_page}&page={page}"
    );
    let jq = r#".items[] | {number, title, author: (.user.login // "ghost"), created_at, labels: [.labels[] | {name: .name, color: (.color // "8b949e")}], url: .html_url, repo: (.repository_url | split("/") | .[-1]), repo_owner: (.repository_url | split("/") | .[-2])}"#;
    let raw = runner.run(&["api", &endpoint, "--jq", jq]).await?;
    let mut issues = Vec::new();
    let mut first_err: Option<String> = None;
    for line in raw.lines().filter(|l| !l.trim().is_empty()) {
        match serde_json::from_str::<Issue>(line) {
            Ok(issue) => issues.push(issue),
            Err(e) if first_err.is_none() => {
                first_err = Some(format!("parse error: {e}\nraw: {line}"));
            }
            _ => {}
        }
    }
    if issues.is_empty()
        && let Some(err) = first_err
    {
        bail!("{err}");
    }
    debug!(
        "fetch_source_issues: {owner} page={page} -> {} issues",
        issues.len()
    );
    Ok(issues)
}

pub async fn fetch_issue_body(repo: &RepoId, number: u64) -> Result<String> {
    fetch_issue_body_with(&GhCli, repo, number).await
}

pub async fn fetch_issue_body_with<R: GhRunner>(
    runner: &R,
    repo: &RepoId,
    number: u64,
) -> Result<String> {
    let endpoint = format!("{}/issues/{number}", repo.api_base());
    let text = runner
        .run(&["api", &endpoint, "--jq", r#".body // """#])
        .await?;
    Ok(text.trim().to_string())
}

pub async fn fetch_pr_body(
    repo: &RepoId,
    pr_number: u64,
) -> Result<(String, crate::types::MergeableState, u32, u32, String, bool)> {
    fetch_pr_body_with(&GhCli, repo, pr_number).await
}

pub async fn fetch_pr_body_with<R: GhRunner>(
    runner: &R,
    repo: &RepoId,
    pr_number: u64,
) -> Result<(String, crate::types::MergeableState, u32, u32, String, bool)> {
    #[derive(serde::Deserialize)]
    struct Resp {
        body: String,
        mergeable_state: crate::types::MergeableState,
        additions: u32,
        deletions: u32,
        head_sha: String,
        auto_merge: bool,
    }

    debug!("fetch_pr_body: {repo}#{pr_number}");
    let endpoint = format!("{}/pulls/{pr_number}", repo.api_base());
    let raw = runner.run(&["api", &endpoint, "--jq", r#"{body: (.body // ""), mergeable_state: (.mergeable_state // "unknown"), additions: (.additions // 0), deletions: (.deletions // 0), head_sha: .head.sha, auto_merge: (.auto_merge != null)}"#]).await?;
    let resp: Resp = serde_json::from_str(&raw).context("parse pr body response")?;
    Ok((
        resp.body,
        resp.mergeable_state,
        resp.additions,
        resp.deletions,
        resp.head_sha,
        resp.auto_merge,
    ))
}

pub async fn fetch_viewer_permission(repo: &RepoId) -> (bool, bool) {
    fetch_viewer_permission_with(&GhCli, repo).await
}

pub async fn fetch_viewer_permission_with<R: GhRunner>(runner: &R, repo: &RepoId) -> (bool, bool) {
    let endpoint = repo.api_base();
    debug!("gh api {endpoint} --jq {{can_push, allow_auto_merge}}");
    let Ok(text) = runner.run(&["api", &endpoint, "--jq", "{can_push: (.permissions | (.push or .maintain or .admin) // false), allow_auto_merge: (.allow_auto_merge // false)}"]).await else {
        return (false, false);
    };
    #[derive(serde::Deserialize)]
    struct Perm {
        can_push: bool,
        allow_auto_merge: bool,
    }
    serde_json::from_str::<Perm>(&text)
        .map(|p| (p.can_push, p.allow_auto_merge))
        .unwrap_or((false, false))
}

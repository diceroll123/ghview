use anyhow::{Context, Result, bail};
use log::debug;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::Command;

use crate::types::PrId;

pub async fn approve(pr: &PrId) -> Result<()> {
    run_silent(&[
        "pr",
        "review",
        &pr.number.to_string(),
        "--approve",
        "-R",
        &pr.repo.to_string(),
    ])
    .await
}

/// Maximum number of `gh pr merge` attempts for a single PR.
const MERGE_MAX_ATTEMPTS: u32 = 3;

/// Backoff before each retry of a failed merge, by zero-based retry index.
fn merge_retry_delay(retry: u32) -> std::time::Duration {
    // 500ms, 1s: long enough for the base branch to settle after a concurrent merge.
    std::time::Duration::from_millis(500u64.saturating_mul(u64::from(retry + 1)))
}

/// True when a merge failure is the transient "Base branch was modified" error GitHub
/// returns when another PR merged into the same base branch between our state read and
/// the merge mutation. Safe to retry: `gh` re-reads current head/base state on each run,
/// and the error message itself says "Review and try the merge again". See cli/cli#8092.
fn is_base_branch_modified(msg: &str) -> bool {
    msg.to_lowercase().contains("base branch was modified")
}

/// Merges a PR, retrying (with backoff) when GitHub rejects the merge because the base
/// branch moved underneath us. Concurrent merges to the same branch are serialized by
/// the caller; this retry is the safety net for external actors (GitHub's own
/// auto-merge queue, dependabot, other users) and for any residual race. All other
/// errors are returned immediately.
pub async fn merge(pr: &PrId, method: crate::config::MergeMethod, auto: bool) -> Result<()> {
    let number = pr.number.to_string();
    let repo = pr.repo.to_string();
    let mut args: Vec<&str> = vec!["pr", "merge", &number, "-R", &repo];
    if auto {
        args.push("--auto");
    }
    args.push(method.flag());

    let mut attempt = 0;
    loop {
        attempt += 1;
        match run_silent(&args).await {
            Ok(()) => return Ok(()),
            Err(e) if attempt < MERGE_MAX_ATTEMPTS && is_base_branch_modified(&e.to_string()) => {
                let delay = merge_retry_delay(attempt - 1);
                debug!(
                    "gh pr merge {}#{}: base branch was modified, retrying in {:?}",
                    pr.repo, pr.number, delay
                );
                tokio::time::sleep(delay).await;
            }
            Err(e) => return Err(e),
        }
    }
}

pub async fn close_pr(pr: &PrId) -> Result<()> {
    run_silent(&[
        "pr",
        "close",
        &pr.number.to_string(),
        "-R",
        &pr.repo.to_string(),
    ])
    .await
}

pub async fn reopen_pr(pr: &PrId) -> Result<()> {
    run_silent(&[
        "pr",
        "reopen",
        &pr.number.to_string(),
        "-R",
        &pr.repo.to_string(),
    ])
    .await
}

pub async fn mark_ready(pr: &PrId) -> Result<()> {
    run_silent(&[
        "pr",
        "ready",
        &pr.number.to_string(),
        "-R",
        &pr.repo.to_string(),
    ])
    .await
}

pub fn open_url(url: &str) -> Result<()> {
    let cmd = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    debug!("{cmd} {url}");
    tokio::process::Command::new(cmd)
        .arg(url)
        .spawn()
        .context("failed to open url")?;
    Ok(())
}

pub async fn post_comment(pr: &PrId, body: &str) -> Result<()> {
    run_silent(&[
        "pr",
        "comment",
        &pr.number.to_string(),
        "-R",
        &pr.repo.to_string(),
        "--body",
        body,
    ])
    .await
}

/// Runs an interactive gh subcommand (checkout, comment) after TUI suspends.
/// Caller must restore terminal after this returns.
pub fn spawn_interactive(args: &[&str]) -> std::io::Result<std::process::Child> {
    debug!("gh {} (interactive)", args.join(" "));
    std::process::Command::new("gh")
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
}

async fn run_silent(args: &[&str]) -> Result<()> {
    debug!("gh {}", args.join(" "));
    let out = Command::new("gh")
        .args(args)
        .output()
        .await
        .context("failed to run gh")?;

    if out.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&out.stderr);
        debug!("gh {} error: {}", args.join(" "), stderr.trim());
        bail!("{}", stderr.trim());
    }
}

/// Resolves the directory a repo should be cloned into, without creating it.
/// `clone_dir` supports `~` expansion (via shellexpand); `None` defaults to the
/// current directory (`.`). Always joins `owner` onto the base so single-repo
/// clones and org-wide clones land in the same place: `{clone_dir}/{owner}/{repo}`.
pub fn resolve_clone_dir(clone_dir: Option<&str>, owner: &str) -> PathBuf {
    let base = clone_dir.map_or_else(
        || PathBuf::from("."),
        |d| PathBuf::from(shellexpand::tilde(d).into_owned()),
    );
    base.join(owner)
}

/// Like `resolve_clone_dir`, but also creates the directory (and parents) if missing.
pub fn clone_base_dir(clone_dir: Option<&str>, owner: &str) -> std::io::Result<PathBuf> {
    let dir = resolve_clone_dir(clone_dir, owner);
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Blocking: enumerates every repo under `owner` via `gh repo list` and clones each
/// one into `base` (expected to already exist, e.g. from `clone_base_dir`). Must be
/// called only after the TUI has been suspended (stdout/stdin restored to the real
/// terminal), since it prints progress and blocks on a final Enter keypress so the
/// output can be read before the TUI redraws.
///
/// Individual clone failures are soft-reported (printed, then the batch continues).
/// Only a failure of the initial `gh repo list` itself is a hard error.
pub fn run_clone_org(owner: &str, base: &Path) -> Result<()> {
    use std::io::Write;

    debug!("gh repo list {owner} --limit 1000 --json nameWithOwner (interactive)");
    let out = std::process::Command::new("gh")
        .args([
            "repo",
            "list",
            owner,
            "--limit",
            "1000",
            "--json",
            "nameWithOwner",
        ])
        .output()
        .context("failed to run gh repo list")?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        bail!("{}", stderr.trim());
    }

    #[derive(serde::Deserialize)]
    struct RepoListItem {
        #[serde(rename = "nameWithOwner")]
        name_with_owner: String,
    }

    let items: Vec<RepoListItem> =
        serde_json::from_slice(&out.stdout).context("failed to parse gh repo list output")?;

    let total = items.len();
    let mut cloned = 0usize;
    for item in &items {
        let name_with_owner = &item.name_with_owner;
        let repo_name = name_with_owner
            .rsplit('/')
            .next()
            .unwrap_or(name_with_owner);
        println!("==> {name_with_owner}");
        let status = std::process::Command::new("gh")
            .args(["repo", "clone", name_with_owner, repo_name])
            .current_dir(base)
            .status();
        match status {
            Ok(s) if s.success() => cloned += 1,
            Ok(_) => println!("FAILED: {name_with_owner}"),
            Err(e) => println!("FAILED: {name_with_owner} ({e})"),
        }
    }

    println!("\nCloned {cloned}/{total} repos.");
    print!("Press Enter to continue...");
    let _ = std::io::stdout().flush();
    let mut buf = String::new();
    let _ = std::io::stdin().read_line(&mut buf);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_clone_dir_none_defaults_to_cwd() {
        assert_eq!(
            resolve_clone_dir(None, "acme"),
            PathBuf::from(".").join("acme")
        );
    }

    #[test]
    fn resolve_clone_dir_joins_owner_onto_base() {
        assert_eq!(
            resolve_clone_dir(Some("/tmp/somebase"), "acme"),
            PathBuf::from("/tmp/somebase").join("acme")
        );
    }

    #[test]
    fn is_base_branch_modified_matches_gh_error() {
        assert!(is_base_branch_modified(
            "GraphQL: Base branch was modified. Review and try the merge again. (mergePullRequest)"
        ));
    }

    #[test]
    fn is_base_branch_modified_is_case_insensitive() {
        assert!(is_base_branch_modified("base branch was modified"));
        assert!(is_base_branch_modified("BASE BRANCH WAS MODIFIED"));
    }

    #[test]
    fn is_base_branch_modified_rejects_other_errors() {
        assert!(!is_base_branch_modified(
            "GraphQL: Pull Request is not mergeable (mergePullRequest)"
        ));
        assert!(!is_base_branch_modified(
            "branch was modified by someone else"
        ));
        assert!(!is_base_branch_modified(""));
    }

    #[test]
    fn merge_retry_delay_grows_with_attempt() {
        assert_eq!(merge_retry_delay(0), std::time::Duration::from_millis(500));
        assert_eq!(merge_retry_delay(1), std::time::Duration::from_millis(1000));
    }

    #[test]
    fn clone_base_dir_creates_directory() {
        let temp_base = std::env::temp_dir().join(format!("ghview-test-{}", std::process::id()));
        let temp_base_str = temp_base.to_str().expect("temp path is valid utf-8");
        let dir = clone_base_dir(Some(temp_base_str), "acme").expect("clone_base_dir failed");
        assert!(dir.is_dir());
        let _ = std::fs::remove_dir_all(&temp_base);
    }
}

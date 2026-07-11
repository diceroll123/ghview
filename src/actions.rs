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

pub async fn merge(pr: &PrId, method: crate::config::MergeMethod, auto: bool) -> Result<()> {
    let number = pr.number.to_string();
    let repo = pr.repo.to_string();
    let mut args = vec!["pr", "merge", &number, "-R", &repo];
    if auto {
        args.push("--auto");
    }
    args.push(method.flag());
    run_silent(&args).await
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
    fn clone_base_dir_creates_directory() {
        let temp_base = std::env::temp_dir().join(format!("ghview-test-{}", std::process::id()));
        let temp_base_str = temp_base.to_str().expect("temp path is valid utf-8");
        let dir = clone_base_dir(Some(temp_base_str), "acme").expect("clone_base_dir failed");
        assert!(dir.is_dir());
        let _ = std::fs::remove_dir_all(&temp_base);
    }
}

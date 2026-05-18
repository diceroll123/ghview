use anyhow::{Result, bail};
use std::process::Stdio;
use tokio::process::Command;

pub async fn approve(org: &str, repo: &str, pr: u64) -> Result<()> {
    run_silent(&[
        "pr",
        "review",
        &pr.to_string(),
        "--approve",
        "-R",
        &format!("{org}/{repo}"),
    ])
    .await
}

pub async fn merge(
    org: &str,
    repo: &str,
    pr: u64,
    method: crate::config::MergeMethod,
) -> Result<()> {
    run_silent(&[
        "pr",
        "merge",
        &pr.to_string(),
        "-R",
        &format!("{org}/{repo}"),
        "--auto",
        method.flag(),
    ])
    .await
}

pub async fn close_pr(org: &str, repo: &str, pr: u64) -> Result<()> {
    run_silent(&[
        "pr",
        "close",
        &pr.to_string(),
        "-R",
        &format!("{org}/{repo}"),
    ])
    .await
}

pub async fn reopen_pr(org: &str, repo: &str, pr: u64) -> Result<()> {
    run_silent(&[
        "pr",
        "reopen",
        &pr.to_string(),
        "-R",
        &format!("{org}/{repo}"),
    ])
    .await
}

pub async fn mark_ready(org: &str, repo: &str, pr: u64) -> Result<()> {
    run_silent(&[
        "pr",
        "ready",
        &pr.to_string(),
        "-R",
        &format!("{org}/{repo}"),
    ])
    .await
}

pub fn open_url(url: &str) -> Result<()> {
    use anyhow::Context;
    let cmd = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    tokio::process::Command::new(cmd)
        .arg(url)
        .spawn()
        .context("failed to open url")?;
    Ok(())
}

pub async fn post_comment(org: &str, repo: &str, pr: u64, body: &str) -> Result<()> {
    run_silent(&[
        "pr",
        "comment",
        &pr.to_string(),
        "-R",
        &format!("{org}/{repo}"),
        "--body",
        body,
    ])
    .await
}

/// Runs an interactive gh subcommand (checkout, comment) after TUI suspends.
/// Caller must restore terminal after this returns.
pub fn spawn_interactive(args: &[&str]) -> std::io::Result<std::process::Child> {
    std::process::Command::new("gh")
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
}

async fn run_silent(args: &[&str]) -> Result<()> {
    use anyhow::Context;
    let out = Command::new("gh")
        .args(args)
        .output()
        .await
        .context("failed to run gh")?;

    if out.status.success() {
        Ok(())
    } else {
        bail!("{}", String::from_utf8_lossy(&out.stderr).trim());
    }
}

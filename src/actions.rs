use anyhow::{Result, bail};
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

pub async fn merge(pr: &PrId, method: crate::config::MergeMethod) -> Result<()> {
    run_silent(&[
        "pr",
        "merge",
        &pr.number.to_string(),
        "-R",
        &pr.repo.to_string(),
        "--auto",
        method.flag(),
    ])
    .await
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

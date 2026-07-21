use clap::Parser;
use color_eyre::Result;
use crossterm::{
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ghview::app::{App, InteractiveCmd, InteractiveKind, run_event_loop};
use ghview::types::RepoId;
use log::debug;
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io;
use tokio::sync::mpsc::unbounded_channel;

#[derive(Debug, Clone, PartialEq, Eq)]
enum LaunchTarget {
    Repo(RepoId),
    Owner(String),
}

fn parse_launch_arg(s: &str) -> Result<LaunchTarget, String> {
    if let Some((owner, repo)) = s.split_once('/') {
        if owner.is_empty() || repo.is_empty() || repo.contains('/') {
            return Err(format!("invalid repo \"{s}\": expected OWNER/REPO"));
        }
        return Ok(LaunchTarget::Repo(RepoId::new(owner, repo)));
    }
    if s.is_empty() {
        return Err("expected OWNER/REPO or OWNER, got empty string".to_string());
    }
    Ok(LaunchTarget::Owner(s.to_string()))
}

#[derive(Parser)]
#[command(name = "ghview", about = "GitHub PR browser")]
struct Args {
    /// Open directly into a repo's workspace (OWNER/REPO) or an owner's repo list (OWNER), skipping Sources/Repos browsing
    #[arg(value_name = "OWNER/REPO|OWNER", value_parser = parse_launch_arg)]
    target: Option<LaunchTarget>,

    /// Write debug logs to ./debug.log
    #[arg(long)]
    debug: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;

    let args = Args::parse();
    if args.debug {
        let log_file = std::fs::File::create("debug.log")?;
        let mut builder = simplelog::ConfigBuilder::new();
        builder.set_time_format_custom(time::macros::format_description!(
            "[hour]:[minute]:[second].[subsecond digits:3]"
        ));
        let _ = builder.set_time_offset_to_local();
        simplelog::WriteLogger::init(simplelog::LevelFilter::Debug, builder.build(), log_file).ok();
    }

    let mut stdout = io::stdout();
    enable_raw_mode()?;
    execute!(stdout, EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(&mut terminal, args.target).await;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    direct_target: Option<LaunchTarget>,
) -> Result<()> {
    let cfg = ghview::config::load();
    let (tx, mut rx) = unbounded_channel();
    let mut app = App::new(tx, cfg);
    match direct_target {
        Some(LaunchTarget::Repo(repo)) => app.enter_direct_repo(repo),
        Some(LaunchTarget::Owner(owner)) => app.enter_direct_owner(owner),
        None => app.trigger_load_sources(),
    }

    loop {
        let (cmd, returned_app) = run_event_loop(app, rx, terminal).await?;

        let Some(InteractiveCmd {
            kind,
            repo,
            pr_number,
        }) = cmd
        else {
            break;
        };

        disable_raw_mode()?;
        execute!(io::stdout(), LeaveAlternateScreen)?;

        let mut child = match kind {
            InteractiveKind::Checkout => {
                debug!("gh pr checkout {pr_number} -R {repo} (interactive)");
                let mut cmd = std::process::Command::new("gh");
                cmd.args([
                    "pr",
                    "checkout",
                    &pr_number.to_string(),
                    "-R",
                    &repo.to_string(),
                ]);
                if let Some(dir) = returned_app.config.ui.checkout_dir.as_deref() {
                    let expanded = shellexpand::tilde(dir).into_owned();
                    cmd.current_dir(&expanded);
                }
                cmd.spawn()?
            }
            InteractiveKind::Comment => ghview::actions::spawn_interactive(&[
                "pr",
                "comment",
                &pr_number.to_string(),
                "-R",
                &repo.to_string(),
            ])?,
            InteractiveKind::Custom(cmd) => {
                debug!("sh -c {cmd} (interactive)");
                std::process::Command::new("sh")
                    .arg("-c")
                    .arg(&cmd)
                    .spawn()?
            }
        };
        let _ = child.wait();

        execute!(io::stdout(), EnterAlternateScreen)?;
        enable_raw_mode()?;
        // Reinitialize instead of terminal.clear() - clear() reads cursor position via stdin, timing out if the child left residual bytes there.
        *terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;

        let (new_tx, new_rx) = unbounded_channel();
        app = returned_app.resume(new_tx);
        rx = new_rx;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_launch_arg_owner_repo() {
        assert_eq!(
            parse_launch_arg("owner/repo"),
            Ok(LaunchTarget::Repo(RepoId::new("owner", "repo")))
        );
    }

    #[test]
    fn parse_launch_arg_bare_owner() {
        assert_eq!(
            parse_launch_arg("owner"),
            Ok(LaunchTarget::Owner("owner".to_string()))
        );
    }

    #[test]
    fn parse_launch_arg_empty() {
        assert!(parse_launch_arg("").is_err());
    }

    #[test]
    fn parse_launch_arg_empty_repo_part() {
        assert!(parse_launch_arg("owner/").is_err());
    }

    #[test]
    fn parse_launch_arg_empty_owner_part() {
        assert!(parse_launch_arg("/repo").is_err());
    }

    #[test]
    fn parse_launch_arg_repo_part_has_slash() {
        assert!(parse_launch_arg("owner/repo/extra").is_err());
    }
}

#![deny(clippy::correctness)]
#![warn(clippy::suspicious, clippy::style, clippy::complexity, clippy::perf)]

mod actions;
mod app;
mod config;
mod data;
mod keys;
mod types;
mod ui;

use app::{App, InteractiveCmd, InteractiveKind, run_event_loop};
use clap::Parser;
use color_eyre::Result;
use crossterm::{
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io;
use tokio::sync::mpsc::unbounded_channel;

#[derive(Parser)]
#[command(name = "ghview", about = "GitHub PR browser")]
struct Args {
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
        simplelog::WriteLogger::init(
            simplelog::LevelFilter::Debug,
            simplelog::Config::default(),
            log_file,
        )
        .ok();
    }

    let mut stdout = io::stdout();
    enable_raw_mode()?;
    execute!(stdout, EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(&mut terminal).await;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

async fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    let cfg = config::load();
    let (tx, mut rx) = unbounded_channel();
    let mut app = App::new(tx, cfg);
    app.trigger_load_sources();

    loop {
        let (cmd, returned_app) = run_event_loop(app, rx, terminal).await?;

        let Some(InteractiveCmd {
            kind,
            owner,
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
                let mut cmd = std::process::Command::new("gh");
                cmd.args([
                    "pr",
                    "checkout",
                    &pr_number.to_string(),
                    "-R",
                    &format!("{owner}/{repo}"),
                ]);
                if let Some(dir) = returned_app.config.ui.checkout_dir.as_deref() {
                    let expanded = shellexpand::tilde(dir).into_owned();
                    cmd.current_dir(&expanded);
                }
                cmd.spawn()?
            }
            InteractiveKind::Comment => actions::spawn_interactive(&[
                "pr",
                "comment",
                &pr_number.to_string(),
                "-R",
                &format!("{owner}/{repo}"),
            ])?,
            InteractiveKind::Custom(cmd) => std::process::Command::new("sh")
                .arg("-c")
                .arg(&cmd)
                .spawn()?,
        };
        let _ = child.wait();

        execute!(io::stdout(), EnterAlternateScreen)?;
        enable_raw_mode()?;
        terminal.clear()?;

        let (new_tx, new_rx) = unbounded_channel();
        app = returned_app.resume(new_tx);
        rx = new_rx;
    }

    Ok(())
}

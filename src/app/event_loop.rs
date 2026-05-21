use super::App;
use crate::{
    keys::{Action, builtin_to_action, map_key_checks, map_key_prs, map_key_universal},
    types::{Column, DataMsg, DetailSection, RepoId, RepoView, ReposView},
    ui::draw,
};
use crossterm::event::{Event, EventStream, KeyCode, KeyEventKind};
use futures::StreamExt;
use tokio::time::interval_at;

pub struct InteractiveCmd {
    pub kind: InteractiveKind,
    pub repo: RepoId,
    pub pr_number: u64,
}

pub enum InteractiveKind {
    Checkout,
    Comment,
    Custom(String),
}

pub async fn run_event_loop(
    mut app: App,
    mut rx: tokio::sync::mpsc::UnboundedReceiver<DataMsg>,
    terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
) -> color_eyre::Result<(Option<InteractiveCmd>, App)> {
    let mut events = EventStream::new();
    let mut tick = tokio::time::interval(app.config.tick_interval());
    let mut rate_limit_tick = tokio::time::interval(app.config.rate_limit_refresh_interval());
    let start30 = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
    let mut watch_tick = interval_at(start30, std::time::Duration::from_secs(30));
    app.trigger_fetch_rate_limit();

    loop {
        terminal.draw(|f| draw(f, &mut app))?;

        tokio::select! {
            maybe_event = events.next() => {
                let Some(Ok(Event::Key(key))) = maybe_event else { continue };
                if key.kind != KeyEventKind::Press { continue }

                if app.filter_active {
                    app.handle_filter_input(key);
                    continue;
                }

                if app.show_help {
                    match key.code {
                        KeyCode::Esc                           => { app.show_help = false; app.help_scroll = 0; }
                        KeyCode::Char('j') | KeyCode::Down     => app.help_scroll = app.help_scroll.saturating_add(1),
                        KeyCode::Char('k') | KeyCode::Up       => app.help_scroll = app.help_scroll.saturating_sub(1),
                        KeyCode::Char('g') | KeyCode::Home     => app.help_scroll = 0,
                        KeyCode::Char('G') | KeyCode::End      => app.help_scroll = u16::MAX,
                        _ => app.handle_action(map_key_universal(key).unwrap_or(Action::Help)),
                    }
                    continue;
                }

                if app.show_dependabot_menu {
                    if let KeyCode::Char(c) = key.code {
                        app.handle_dependabot_key(c);
                    } else {
                        app.show_dependabot_menu = false;
                    }
                    continue;
                }

                if app.focus == Column::Detail && key.code == KeyCode::Tab
                    && (app.repo_view == RepoView::Prs || app.repos_view == ReposView::PrList) {
                    app.detail_tab();
                    continue;
                }

                // View switching in the Browse column (p = PR list, r = repo list).
                if app.focus == Column::Repos {
                    match key.code {
                        KeyCode::Char('p') => {
                            app.repos_view = ReposView::PrList;
                            if app.source_ctx.source_prs.is_empty() {
                                app.trigger_load_source_prs();
                            }
                            continue;
                        }
                        KeyCode::Char('r') if app.repos_view == ReposView::PrList => {
                            app.repos_view = ReposView::RepoList;
                            continue;
                        }
                        _ => {}
                    }
                }

                // View switching when in repo workspace (f/p/i).
                if matches!(app.focus, Column::Repo | Column::Detail) {
                    match key.code {
                        KeyCode::Char('f') => { app.switch_repo_view(RepoView::Frontpage); continue; }
                        KeyCode::Char('p') => { app.switch_repo_view(RepoView::Prs); continue; }
                        KeyCode::Char('i') if app.selected_repo_has_issues() => { app.switch_repo_view(RepoView::Issues); continue; }
                        _ => {}
                    }
                }

                // 1. Universal user keybindings — override all defaults.
                if let Some(action) = app.config.keybindings.universal.iter()
                    .find(|kb| kb.matches(key))
                    .and_then(|kb| kb.builtin.as_deref())
                    .and_then(builtin_to_action)
                {
                    app.handle_action(action);
                    if app.should_quit { return Ok((None, app)); }
                    continue;
                }

                // 2. Column user keybindings — repos config before repo defaults.
                if app.focus == Column::Repos {
                    let kb = app.config.keybindings.repos.iter().find(|kb| kb.matches(key)).cloned();
                    if let Some(kb) = kb {
                        if let Some(action) = kb.builtin.as_deref().and_then(builtin_to_action) {
                            app.handle_action(action);
                            if app.should_quit { return Ok((None, app)); }
                            continue;
                        }
                        if let Some(shell_cmd) = app.trigger_keybinding_repo(&kb) {
                            let Some(owner) = app.selected_source_owner() else { continue };
                            let Some(repo) = app.selected_repo().map(std::string::ToString::to_string) else { continue };
                            return Ok((Some(InteractiveCmd {
                                kind: InteractiveKind::Custom(shell_cmd),
                                repo: RepoId::new(owner, repo),
                                pr_number: 0,
                            }), app));
                        }
                        continue;
                    }
                }

                // 2. Column user keybindings — PRs config before PR defaults (repo list and source list).
                if (app.focus == Column::Repo && app.repo_view == crate::types::RepoView::Prs)
                    || (app.focus == Column::Repos
                        && app.repos_view == ReposView::PrList)
                {
                    let kb = app.config.keybindings.prs.iter().find(|kb| kb.matches(key)).cloned();
                    if let Some(kb) = kb {
                        if let Some(action) = kb.builtin.as_deref().and_then(builtin_to_action) {
                            if matches!(action, Action::Checkout | Action::Comment) {
                                let Some((rid, pr)) = app.selected_pr_context() else { continue };
                                let kind = if action == Action::Checkout { InteractiveKind::Checkout } else { InteractiveKind::Comment };
                                return Ok((Some(InteractiveCmd { kind, repo: rid, pr_number: pr.number }), app));
                            }
                            app.handle_action(action);
                            if app.should_quit { return Ok((None, app)); }
                            continue;
                        }
                        if let Some(shell_cmd) = app.trigger_keybinding_pr(&kb) {
                            let Some((rid, pr)) = app.selected_pr_context() else { continue };
                            return Ok((Some(InteractiveCmd {
                                kind: InteractiveKind::Custom(shell_cmd),
                                repo: rid,
                                pr_number: pr.number,
                            }), app));
                        }
                        continue;
                    }
                }

                // 2. Column user keybindings — Checks config before checks defaults.
                // Also apply checks defaults here (before universal defaults) so that
                // Enter/l open the selected check rather than triggering the universal Right action.
                if app.focus == Column::Detail && app.repo_ctx.detail_section == DetailSection::Checks {
                    let kb = app.config.keybindings.checks.iter().find(|kb| kb.matches(key)).cloned();
                    if let Some(kb) = kb {
                        if let Some(action) = kb.builtin.as_deref().and_then(builtin_to_action) {
                            app.handle_action(action);
                            if app.should_quit { return Ok((None, app)); }
                            continue;
                        }
                        if let Some(shell_cmd) = app.trigger_keybinding_check(&kb) {
                            let Some((rid, pr)) = app.selected_pr_context() else { continue };
                            return Ok((Some(InteractiveCmd {
                                kind: InteractiveKind::Custom(shell_cmd),
                                repo: rid,
                                pr_number: pr.number,
                            }), app));
                        }
                        continue;
                    }
                    // Checks-section defaults — run before universal defaults so Enter/l open the check.
                    if let Some(action) = map_key_checks(key) {
                        app.handle_action(action);
                        if app.should_quit { return Ok((None, app)); }
                        continue;
                    }
                }

                // 3. Universal defaults.
                if let Some(action) = map_key_universal(key) {
                    app.handle_action(action);
                    if app.should_quit { return Ok((None, app)); }
                    continue;
                }

                // 4. PR-column defaults (repo list and source list).
                if (app.focus == Column::Repo && app.repo_view == crate::types::RepoView::Prs
                    || app.focus == Column::Repos && app.repos_view == ReposView::PrList)
                    && let Some(action) = map_key_prs(key) {
                        if matches!(action, Action::Checkout | Action::Comment) {
                            let Some((rid, pr)) = app.selected_pr_context() else { continue };
                            let kind = if action == Action::Checkout { InteractiveKind::Checkout } else { InteractiveKind::Comment };
                            return Ok((Some(InteractiveCmd { kind, repo: rid, pr_number: pr.number }), app));
                        }
                        app.handle_action(action);
                        if app.should_quit { return Ok((None, app)); }
                    }
            }

            Some(msg) = rx.recv() => {
                let is_prs = matches!(msg, DataMsg::Prs { .. });
                app.handle_data(msg);
                if is_prs {
                    app.trigger_review_and_check_fetches();
                }
            }

            _ = tick.tick() => { app.clear_status_if_expired(); }

            _ = rate_limit_tick.tick() => {
                app.trigger_fetch_rate_limit();
            }

            _ = watch_tick.tick() => {
                if matches!(app.focus, Column::Repo | Column::Detail)
                    && let Some(pr) = app.selected_pr()
                    && !pr.head_sha.is_empty()
                    && let (Some(owner), Some(repo)) = (
                        app.selected_source_owner(),
                        app.selected_repo().map(str::to_string),
                    )
                {
                    let pr_number = pr.number;
                    let sha = pr.head_sha.clone();
                    let tx = app.tx.clone();
                    let rid = crate::types::RepoId::new(owner, repo);
                    tokio::spawn(async move {
                        let runs = crate::data::fetch_check_runs(&rid, &sha).await;
                        let _ = tx.send(DataMsg::CheckRuns {
                            pr: rid.pr(pr_number),
                            runs,
                        });
                    });
                }
            }
        }
    }
}

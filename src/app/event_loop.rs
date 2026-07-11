use super::App;
use crate::{
    config::Keybinding,
    keys::{
        Action, CHECKS_BINDINGS, DefaultBinding, ISSUES_BINDINGS, PRS_BINDINGS, REPOS_BINDINGS,
        UNIVERSAL_BINDINGS, builtin_to_action, map_key_universal,
    },
    types::{Column, DataMsg, DetailSection, RepoId, RepoView, ReposView},
    ui::draw,
};
use crossterm::event::{Event, EventStream, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use futures::StreamExt;
use tokio::time::interval_at;

#[derive(Clone, Copy, PartialEq, Eq)]
enum LayerKind {
    Checks,
    Issues,
    Prs,
    Repos,
    Universal,
}

struct KeyLayer<'a> {
    kind: LayerKind,
    user: &'a [Keybinding],
    defaults: &'static [DefaultBinding],
}

enum InputContext {
    ChecksDetail, // checks section in detail panel; also inherits PR keys
    PrContext,    // PR list, source PR list, or PR detail
    IssueContext, // issue list (per-repo or source-level) or issue detail
    Repos,        // repos column
    Generic,      // sources, frontpage
}

impl InputContext {
    fn from_app(app: &App) -> Self {
        if app.focus == Column::Detail && app.repo_ctx.detail_section == DetailSection::Checks {
            Self::ChecksDetail
        } else if (matches!(app.focus, Column::Repo | Column::Detail)
            && app.repo_view == RepoView::Prs)
            || (matches!(app.focus, Column::Repos | Column::Detail)
                && app.repos_view == ReposView::PrList)
        {
            Self::PrContext
        } else if (matches!(app.focus, Column::Repo | Column::Detail)
            && app.repo_view == RepoView::Issues)
            || (matches!(app.focus, Column::Repos | Column::Detail)
                && app.repos_view == ReposView::IssueList)
        {
            Self::IssueContext
        } else if app.focus == Column::Repos {
            Self::Repos
        } else {
            Self::Generic
        }
    }
}

fn active_layers(app: &App) -> Vec<KeyLayer<'_>> {
    let kb = &app.config.keybindings;
    let universal = KeyLayer {
        kind: LayerKind::Universal,
        user: &kb.universal,
        defaults: UNIVERSAL_BINDINGS,
    };
    let prs = KeyLayer {
        kind: LayerKind::Prs,
        user: &kb.prs,
        defaults: PRS_BINDINGS,
    };
    let repos = KeyLayer {
        kind: LayerKind::Repos,
        user: &kb.repos,
        defaults: REPOS_BINDINGS,
    };

    match InputContext::from_app(app) {
        InputContext::ChecksDetail => vec![
            KeyLayer {
                kind: LayerKind::Checks,
                user: &kb.checks,
                defaults: CHECKS_BINDINGS,
            },
            prs,
            universal,
        ],
        // When Column::Repos is focused in PrList or IssueList mode, prepend a view-switch
        // layer so r/p/i still work to switch tabs.
        InputContext::PrContext if app.focus == Column::Repos => vec![
            KeyLayer {
                kind: LayerKind::Repos,
                user: &[],
                defaults: REPOS_BINDINGS,
            },
            prs,
            universal,
        ],
        InputContext::PrContext => vec![prs, universal],
        InputContext::IssueContext if app.focus == Column::Repos => vec![
            KeyLayer {
                kind: LayerKind::Repos,
                user: &[],
                defaults: REPOS_BINDINGS,
            },
            KeyLayer {
                kind: LayerKind::Issues,
                user: &kb.issues,
                defaults: ISSUES_BINDINGS,
            },
            universal,
        ],
        InputContext::IssueContext => vec![
            KeyLayer {
                kind: LayerKind::Issues,
                user: &kb.issues,
                defaults: ISSUES_BINDINGS,
            },
            universal,
        ],
        InputContext::Repos => vec![repos, universal],
        InputContext::Generic => vec![universal],
    }
}

enum LayerMatch {
    Action(Action),
    Shell(LayerKind, Keybinding),
    Consumed, // user binding matched but nothing to run; still eats the key
}

// Separate from dispatch so the shared borrow on app is dropped before mutation.
fn find_layer_match(key: KeyEvent, app: &App) -> Option<LayerMatch> {
    for layer in active_layers(app) {
        if let Some(kb) = layer.user.iter().find(|kb| kb.matches(key)).cloned() {
            if let Some(action) = kb.builtin.as_deref().and_then(builtin_to_action) {
                return Some(LayerMatch::Action(action));
            }
            return Some(if kb.command.is_some() {
                LayerMatch::Shell(layer.kind, kb)
            } else {
                LayerMatch::Consumed
            });
        }
        if let Some(b) = layer.defaults.iter().find(|b| b.keys.contains(&key.code)) {
            return Some(LayerMatch::Action(b.action));
        }
    }
    None
}

enum DispatchResult {
    Handled,
    Quit,
    Interactive(InteractiveCmd),
}

fn dispatch_action(action: Action, app: &mut App) -> Option<DispatchResult> {
    match action {
        Action::Checkout | Action::Comment => {
            let (rid, pr) = app.selected_pr_context()?;
            let kind = if action == Action::Checkout {
                InteractiveKind::Checkout
            } else {
                InteractiveKind::Comment
            };
            Some(DispatchResult::Interactive(InteractiveCmd {
                kind,
                repo: rid,
                pr_number: pr.number,
            }))
        }
        Action::Clone => match app.focus {
            Column::Sources => {
                let owner = app.selected_source_owner()?;
                app.pending_clone_org = Some(owner);
                Some(DispatchResult::Handled)
            }
            _ => {
                let rid = app.selected_owner_repo()?;
                Some(DispatchResult::Interactive(InteractiveCmd {
                    kind: InteractiveKind::Clone,
                    repo: rid,
                    pr_number: 0,
                }))
            }
        },
        _ => {
            app.handle_action(action);
            Some(if app.should_quit {
                DispatchResult::Quit
            } else {
                DispatchResult::Handled
            })
        }
    }
}

fn dispatch_key(key: KeyEvent, app: &mut App) -> Option<DispatchResult> {
    match find_layer_match(key, app)? {
        LayerMatch::Action(action) => dispatch_action(action, app),
        LayerMatch::Consumed => Some(DispatchResult::Handled),
        LayerMatch::Shell(kind, kb) => match kind {
            LayerKind::Repos => app.trigger_keybinding_repo(&kb).map(|cmd| {
                let owner = app.selected_source_owner().unwrap_or_default();
                let repo = app.selected_repo().map(str::to_string).unwrap_or_default();
                DispatchResult::Interactive(InteractiveCmd {
                    kind: InteractiveKind::Custom(cmd),
                    repo: RepoId::new(owner, repo),
                    pr_number: 0,
                })
            }),
            LayerKind::Issues => app.trigger_keybinding_issue(&kb).and_then(|cmd| {
                app.selected_issue_context().map(|(rid, _)| {
                    DispatchResult::Interactive(InteractiveCmd {
                        kind: InteractiveKind::Custom(cmd),
                        repo: rid,
                        pr_number: 0,
                    })
                })
            }),
            LayerKind::Checks | LayerKind::Prs => {
                let cmd = if kind == LayerKind::Checks {
                    app.trigger_keybinding_check(&kb)
                } else {
                    app.trigger_keybinding_pr(&kb)
                };
                cmd.and_then(|cmd| {
                    app.selected_pr_context().map(|(rid, pr)| {
                        DispatchResult::Interactive(InteractiveCmd {
                            kind: InteractiveKind::Custom(cmd),
                            repo: rid,
                            pr_number: pr.number,
                        })
                    })
                })
            }
            LayerKind::Universal => None,
        },
    }
}

/// Handles a keypress while `app.pending_clone_org` is set (the bulk-clone confirm
/// overlay is showing). Always clears `pending_clone_org`. Returns `Some` only when
/// the user confirmed with 'y'/'Y'; any other key cancels and returns `None`.
fn handle_pending_clone_confirm(key: KeyEvent, app: &mut App) -> Option<InteractiveCmd> {
    let owner = app.pending_clone_org.take()?;
    if !matches!(key.code, KeyCode::Char('y' | 'Y')) {
        return None;
    }
    Some(InteractiveCmd {
        kind: InteractiveKind::CloneOrg(owner.clone()),
        repo: RepoId::new(owner, String::new()),
        pr_number: 0,
    })
}

pub struct InteractiveCmd {
    pub kind: InteractiveKind,
    pub repo: RepoId,
    pub pr_number: u64,
}

pub enum InteractiveKind {
    Checkout,
    Comment,
    Custom(String),
    Clone,
    CloneOrg(String), // owner
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

                if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                    return Ok((None, app));
                }

                if app.pending_clone_org.is_some() {
                    if let Some(cmd) = handle_pending_clone_confirm(key, &mut app) {
                        return Ok((Some(cmd), app));
                    }
                    continue;
                }

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

                if app.focus == Column::Detail
                    && key.code == KeyCode::Tab
                    && (app.repo_view == RepoView::Prs || app.repos_view == ReposView::PrList)
                {
                    app.detail_tab();
                    continue;
                }

                // View switching when in repo workspace (f/p/i).
                if matches!(app.focus, Column::Repo | Column::Detail) {
                    match key.code {
                        KeyCode::Char('f') => { app.try_switch_repo_view(RepoView::Frontpage); continue; }
                        KeyCode::Char('p') => { app.try_switch_repo_view(RepoView::Prs); continue; }
                        KeyCode::Char('i') => { app.try_switch_repo_view(RepoView::Issues); continue; }
                        _ => {}
                    }
                }

                match dispatch_key(key, &mut app) {
                    Some(DispatchResult::Quit) => return Ok((None, app)),
                    Some(DispatchResult::Interactive(cmd)) => return Ok((Some(cmd), app)),
                    Some(DispatchResult::Handled) | None => {}
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn make_app() -> App {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        App::new(tx, Config::default())
    }

    #[test]
    fn pending_clone_confirm_non_confirm_key_cancels_and_clears() {
        let mut app = make_app();
        app.pending_clone_org = Some("acme".into());
        let key = KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE);
        assert!(handle_pending_clone_confirm(key, &mut app).is_none());
        assert!(app.pending_clone_org.is_none());
    }

    #[test]
    fn pending_clone_confirm_esc_cancels_and_clears() {
        let mut app = make_app();
        app.pending_clone_org = Some("acme".into());
        let key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        assert!(handle_pending_clone_confirm(key, &mut app).is_none());
        assert!(app.pending_clone_org.is_none());
    }

    #[test]
    fn pending_clone_confirm_lowercase_y_confirms() {
        let mut app = make_app();
        app.pending_clone_org = Some("acme".into());
        let key = KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE);
        let cmd = handle_pending_clone_confirm(key, &mut app).expect("should confirm");
        assert!(matches!(cmd.kind, InteractiveKind::CloneOrg(ref o) if o == "acme"));
        assert!(app.pending_clone_org.is_none());
    }

    #[test]
    fn pending_clone_confirm_uppercase_y_confirms() {
        let mut app = make_app();
        app.pending_clone_org = Some("acme".into());
        let key = KeyEvent::new(KeyCode::Char('Y'), KeyModifiers::SHIFT);
        let cmd = handle_pending_clone_confirm(key, &mut app).expect("should confirm");
        assert!(matches!(cmd.kind, InteractiveKind::CloneOrg(ref o) if o == "acme"));
    }

    #[test]
    fn pending_clone_confirm_none_pending_returns_none() {
        let mut app = make_app();
        app.pending_clone_org = None;
        let key = KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE);
        assert!(handle_pending_clone_confirm(key, &mut app).is_none());
    }
}

use crate::{
    app::App,
    keys::{
        Action, CHECKS_BAR, DIFF_HINT_TEXT, FRONTPAGE_BAR, ISSUES_BAR, PRS_BAR, REPOS_BAR,
        SOURCES_BAR, find_binding,
    },
    types::{Column, DetailSection, LoadingKind, RepoView},
};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use rattles::presets::prelude as presets;
use std::borrow::Cow;

pub(super) fn draw_status<'a>(f: &mut Frame, app: &'a App, area: ratatui::layout::Rect) {
    let rl_text = app.rate_limit.map(|(rem, lim)| format!("⚡{rem}/{lim}"));
    let rl_width = rl_text.as_ref().map_or(0, |s| {
        u16::try_from(s.len()).unwrap_or(u16::MAX).saturating_add(1)
    });
    let hint_width = area.width.saturating_sub(rl_width) as usize;

    let (hint, hint_color, hint_align): (Cow<'a, str>, Color, Alignment) =
        if app.diff_view.is_some() {
            (
                Cow::Borrowed(DIFF_HINT_TEXT),
                Color::DarkGray,
                Alignment::Left,
            )
        } else if app.filter_active {
            (
                Cow::Owned(format!(
                    "Filter: {}  •  Enter confirm  •  Esc clear",
                    app.active_filter()
                )),
                Color::DarkGray,
                Alignment::Left,
            )
        } else if let Some(kind) = &app.loading {
            let label = match kind {
                LoadingKind::Sources => "loading sources",
                LoadingKind::Repos => "loading repos",
                LoadingKind::Prs => "loading PRs",
                LoadingKind::Issues => "loading issues",
                LoadingKind::Action(name) => name.as_str(),
            };
            (
                Cow::Owned(format!("{}  {}…", presets::dots().current_frame(), label)),
                Color::Yellow,
                Alignment::Left,
            )
        } else if let Some((msg, is_err)) = &app.status_msg {
            let color = if *is_err { Color::Red } else { Color::DarkGray };
            (Cow::Borrowed(msg.as_str()), color, Alignment::Center)
        } else {
            (
                Cow::Owned(hint_entries(app, hint_width)),
                Color::DarkGray,
                Alignment::Left,
            )
        };

    if let Some(rl) = rl_text {
        let rl_color = if let Some((rem, _)) = app.rate_limit {
            if rem < 100 {
                Color::Red
            } else if rem < 500 {
                Color::Yellow
            } else {
                Color::DarkGray
            }
        } else {
            Color::DarkGray
        };
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(0), Constraint::Length(rl_width)])
            .split(area);
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(hint, Style::new().fg(hint_color))))
                .alignment(hint_align),
            chunks[0],
        );
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(rl, Style::new().fg(rl_color)))),
            chunks[1],
        );
    } else {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(hint, Style::new().fg(hint_color))))
                .alignment(hint_align),
            area,
        );
    }
}

fn hint_entries(app: &App, width: usize) -> String {
    let bar = match app.focus {
        Column::Sources => SOURCES_BAR,
        Column::Repos => REPOS_BAR,
        Column::Repo => match app.repo_view {
            RepoView::Frontpage => FRONTPAGE_BAR,
            RepoView::Prs => PRS_BAR,
            RepoView::Issues => ISSUES_BAR,
        },
        Column::Detail => {
            if app.detail_section == DetailSection::Checks {
                CHECKS_BAR
            } else {
                PRS_BAR
            }
        }
    };

    let help_str = find_binding(Action::Help)
        .map(|b| format!("{} {}", b.display, b.label))
        .unwrap_or_default();
    let reserved = if help_str.is_empty() {
        0
    } else {
        help_str.len() + 2
    };
    let budget = width.saturating_sub(reserved);

    let col_kbs: &[crate::config::Keybinding] = match app.focus {
        Column::Repos => &app.config.keybindings.repos,
        Column::Repo => match app.repo_view {
            RepoView::Prs => &app.config.keybindings.prs,
            _ => &[],
        },
        Column::Detail => {
            if app.detail_section == DetailSection::Checks {
                &app.config.keybindings.checks
            } else {
                &app.config.keybindings.prs
            }
        }
        Column::Sources => &[],
    };
    let cap = bar.len() + app.config.keybindings.universal.len() + col_kbs.len();
    let mut candidates: Vec<String> = Vec::with_capacity(cap);
    candidates.extend(bar.iter().filter_map(|&action| {
        if action == Action::Help {
            return None;
        }
        if app.focus == Column::Repo
            && action == Action::DependabotMenu
            && !app.selected_pr_is_dependabot()
        {
            return None;
        }
        find_binding(action).map(|b| format!("{} {}", b.display, b.label))
    }));
    for kb in &app.config.keybindings.universal {
        if let Some(name) = &kb.name {
            candidates.push(format!("{} {}", kb.key, name));
        }
    }
    for kb in col_kbs {
        if let Some(name) = &kb.name {
            candidates.push(format!("{} {}", kb.key, name));
        }
    }

    let sep = "  ";
    let mut parts: Vec<String> = Vec::with_capacity(candidates.len() + 1);
    let mut used = 0usize;
    for entry in candidates {
        let needed = if parts.is_empty() {
            entry.len()
        } else {
            sep.len() + entry.len()
        };
        if used + needed > budget {
            break;
        }
        used += needed;
        parts.push(entry);
    }

    if !help_str.is_empty() {
        parts.push(help_str);
    }
    parts.join(sep)
}

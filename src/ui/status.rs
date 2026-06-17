use crate::{
    app::App,
    keys::{
        Action, CHECKS_AND_PRS_BAR, CHECKS_BAR, DIFF_HINT_TEXT, FRONTPAGE_BAR, ISSUES_BAR, PRS_BAR,
        REPOS_BAR, SOURCES_BAR, find_binding,
    },
    types::{Column, DetailSection, LoadingKind, RepoView, ReposView},
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
        if app.repo_ctx.diff_view.is_some() {
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
                LoadingKind::Frontpage => "loading frontpage",
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
            let color = if *is_err { Color::Red } else { Color::Green };
            (Cow::Borrowed(msg.as_str()), color, Alignment::Left)
        } else {
            (
                Cow::Owned(hint_entries(app, hint_width)),
                Color::DarkGray,
                Alignment::Left,
            )
        };

    if let Some(rl) = rl_text {
        let rl_color = if let Some((rem, _)) = app.rate_limit {
            let target: (u8, u8, u8) = if rem < 100 {
                (180, 30, 30)
            } else if rem < 500 {
                (160, 130, 0)
            } else {
                (85, 85, 85)
            };
            let flash = app.config.ui.rate_limit_flash_secs;
            let t = if flash > 0.0 {
                app.rate_limit_updated_at
                    .map(|at| (at.elapsed().as_secs_f32() / flash).min(1.0))
                    .unwrap_or(1.0)
            } else {
                1.0
            };
            let r = (255.0 + (target.0 as f32 - 255.0) * t) as u8;
            let g = (255.0 + (target.1 as f32 - 255.0) * t) as u8;
            let b = (0.0 + (target.2 as f32) * t) as u8;
            Color::Rgb(r, g, b)
        } else {
            Color::DarkGray
        };
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(0), Constraint::Length(rl_width)])
            .split(area);
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::raw(" "),
                Span::styled(hint, Style::new().fg(hint_color)),
            ]))
            .alignment(hint_align),
            chunks[0],
        );
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(rl, Style::new().fg(rl_color)))),
            chunks[1],
        );
    } else {
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::raw(" "),
                Span::styled(hint, Style::new().fg(hint_color)),
            ]))
            .alignment(hint_align),
            area,
        );
    }
}

fn hint_entries(app: &App, width: usize) -> String {
    let bar = match app.focus {
        Column::Sources => SOURCES_BAR,
        Column::Repos => {
            if app.repos_view == ReposView::PrList {
                PRS_BAR
            } else {
                REPOS_BAR
            }
        }
        Column::Repo => match app.repo_view {
            RepoView::Frontpage => FRONTPAGE_BAR,
            RepoView::Prs => PRS_BAR,
            RepoView::Issues => ISSUES_BAR,
        },
        Column::Detail => {
            if app.repo_ctx.detail_section == DetailSection::Checks
                && (app.repo_view == RepoView::Prs || app.repos_view == ReposView::PrList)
            {
                CHECKS_AND_PRS_BAR
            } else if app.repo_ctx.detail_section == DetailSection::Checks {
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
        Column::Repos => {
            if app.repos_view == ReposView::PrList {
                &app.config.keybindings.prs
            } else {
                &app.config.keybindings.repos
            }
        }
        Column::Repo => match app.repo_view {
            RepoView::Prs => &app.config.keybindings.prs,
            _ => &[],
        },
        Column::Detail => {
            if app.repo_ctx.detail_section == DetailSection::Checks {
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
        let pr_column = matches!(app.focus, Column::Repo | Column::Detail)
            || (app.focus == Column::Repos && app.repos_view == ReposView::PrList);
        if pr_column && !app.action_permitted(action) {
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

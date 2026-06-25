use crate::{
    app::App,
    keys::{
        Action, CHECKS_AND_PRS_BAR, CHECKS_BAR, DIFF_HINT_TEXT, FRONTPAGE_BAR, ISSUES_BAR, PRS_BAR,
        REPOS_BAR, SOURCE_ISSUES_BAR, SOURCE_PRS_BAR, SOURCES_BAR, find_binding,
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
        let color = if let Some((rem, _)) = app.rate_limit {
            if rem < 100 {
                Color::Rgb(180, 30, 30)
            } else if rem < 500 {
                Color::Rgb(160, 130, 0)
            } else {
                Color::Rgb(85, 85, 85)
            }
        } else {
            Color::DarkGray
        };
        const SHIMMER_SECS: f32 = 2.0;
        let phase = app
            .rate_limit_updated_at
            .map(|at| (at.elapsed().as_secs_f32() / SHIMMER_SECS).min(1.0))
            .unwrap_or(1.0);
        let rl_spans: Vec<Span> = if phase < 1.0 {
            tui_shimmer::shimmer_spans_with_style_at_phase(&rl, Style::new().fg(color), phase)
        } else {
            vec![Span::styled(rl, Style::new().fg(color))]
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
        f.render_widget(Paragraph::new(Line::from(rl_spans)), chunks[1]);
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
        Column::Repos => match app.repos_view {
            ReposView::PrList => SOURCE_PRS_BAR,
            ReposView::IssueList => SOURCE_ISSUES_BAR,
            ReposView::RepoList => REPOS_BAR,
        },
        Column::Repo => match app.repo_view {
            RepoView::Frontpage => FRONTPAGE_BAR,
            RepoView::Prs => PRS_BAR,
            RepoView::Issues => ISSUES_BAR,
        },
        Column::Detail => match app.repo_ctx.detail_section {
            DetailSection::Checks => match (app.repo_view, app.repos_view) {
                (RepoView::Prs, _) | (_, ReposView::PrList) => CHECKS_AND_PRS_BAR,
                (
                    RepoView::Frontpage | RepoView::Issues,
                    ReposView::RepoList | ReposView::IssueList,
                ) => CHECKS_BAR,
            },
            DetailSection::Body => match (app.repo_view, app.repos_view) {
                (RepoView::Issues, _) | (_, ReposView::IssueList) => ISSUES_BAR,
                (RepoView::Frontpage | RepoView::Prs, ReposView::RepoList | ReposView::PrList) => {
                    PRS_BAR
                }
            },
        },
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
        Column::Repos => match app.repos_view {
            ReposView::PrList => &app.config.keybindings.prs,
            ReposView::IssueList => &app.config.keybindings.issues,
            ReposView::RepoList => &app.config.keybindings.repos,
        },
        Column::Repo => match app.repo_view {
            RepoView::Prs => &app.config.keybindings.prs,
            RepoView::Issues => &app.config.keybindings.issues,
            RepoView::Frontpage => &[],
        },
        Column::Detail => match app.repo_ctx.detail_section {
            DetailSection::Checks => &app.config.keybindings.checks,
            DetailSection::Body => match (app.repo_view, app.repos_view) {
                (RepoView::Issues, _) | (_, ReposView::IssueList) => &app.config.keybindings.issues,
                (RepoView::Frontpage | RepoView::Prs, ReposView::RepoList | ReposView::PrList) => {
                    &app.config.keybindings.prs
                }
            },
        },
        Column::Sources => &[],
    };
    let cap = bar.len() + app.config.keybindings.universal.len() + col_kbs.len();
    let mut candidates: Vec<String> = Vec::with_capacity(cap);
    let active_view_action = (app.focus == Column::Repos).then(|| app.repos_view.switch_action());
    candidates.extend(bar.iter().filter_map(|&action| {
        if action == Action::Help || Some(action) == active_view_action {
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
        find_binding(action).map(|b| {
            let label = if action == Action::Merge {
                if app.merge_uses_auto() {
                    "auto-merge"
                } else {
                    "merge"
                }
            } else {
                b.label
            };
            format!("{} {}", b.display, label)
        })
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

#![allow(clippy::redundant_pub_crate)]
mod markdown;
mod overlays;
mod panels;
mod status;

use crate::{
    app::App,
    types::{Column, RepoView},
};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Scrollbar, ScrollbarOrientation, ScrollbarState},
};
use unicode_width::UnicodeWidthStr;

// Nerd Font glyphs
pub(super) const ICON_USER: &str = "\u{f007} ";
pub(super) const ICON_ORG: &str = "\u{f0af} ";
pub(super) const ICON_REPO: &str = "\u{e702} ";
pub(super) const ICON_CLOCK: &str = "\u{f017}";
pub(super) const ICON_CLOCK_UPDATED: &str = "\u{f520}";

pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();
    app.terminal_height = area.height;

    let chunks = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(area);

    let main_area = chunks[0];
    let status_area = chunks[1];

    if matches!(app.focus, Column::Repo | Column::Detail) {
        match app.repo_view {
            RepoView::Frontpage => {
                let cols = Layout::horizontal([
                    Constraint::Length(4),
                    Constraint::Length(4),
                    Constraint::Fill(1),
                ])
                .split(main_area);
                panels::draw_sources_strip(f, app, cols[0]);
                panels::draw_repos_strip(f, app, cols[1]);
                panels::draw_repo_frontpage(f, app, cols[2]);
            }
            RepoView::Prs => {
                let cols = Layout::horizontal([
                    Constraint::Length(4),
                    Constraint::Length(4),
                    Constraint::Fill(4),
                    Constraint::Fill(3),
                ])
                .split(main_area);
                panels::draw_sources_strip(f, app, cols[0]);
                panels::draw_repos_strip(f, app, cols[1]);
                panels::draw_prs(f, app, cols[2]);
                panels::draw_pr_detail(f, app, cols[3]);
            }
            RepoView::Issues => {
                let cols = Layout::horizontal([
                    Constraint::Length(4),
                    Constraint::Length(4),
                    Constraint::Fill(4),
                    Constraint::Fill(3),
                ])
                .split(main_area);
                panels::draw_sources_strip(f, app, cols[0]);
                panels::draw_repos_strip(f, app, cols[1]);
                panels::draw_issues(f, app, cols[2]);
                panels::draw_issue_detail(f, app, cols[3]);
            }
        }
    } else {
        let (src_pct, repos_pct, prs_pct) = match app.focus {
            Column::Sources => (30, 30, 40),
            Column::Repos => (15, 38, 47),
            Column::Repo | Column::Detail => unreachable!(),
        };
        let cols = Layout::horizontal([
            Constraint::Percentage(src_pct),
            Constraint::Percentage(repos_pct),
            Constraint::Percentage(prs_pct),
        ])
        .split(main_area);
        panels::draw_sources(f, app, cols[0]);
        panels::draw_repos(f, app, cols[1]);
        panels::draw_prs(f, app, cols[2]);
    }
    status::draw_status(f, app, status_area);

    if app.show_help {
        overlays::draw_help(f, app, area);
    }
    if app.show_dependabot_menu {
        overlays::draw_dependabot_menu(f, area);
    }
    if app.diff_view.is_some() {
        overlays::draw_diff(f, app, area);
    }
}

pub(super) fn render_list_scrollbar(
    f: &mut Frame,
    sb_area: Rect,
    total: usize,
    avail_height: u16,
    position: usize,
) {
    if total > avail_height as usize {
        let mut sb = ScrollbarState::new(total).position(position);
        f.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight),
            sb_area,
            &mut sb,
        );
    }
}

pub(super) fn panel_focus(focused: bool) -> Style {
    if focused {
        active_style()
    } else {
        inactive_style()
    }
}

#[inline]
pub(super) fn active_style() -> Style {
    Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD)
}

#[inline]
pub(super) fn inactive_style() -> Style {
    Style::new().fg(Color::DarkGray)
}

#[inline]
pub(super) fn item_style(focused: bool) -> Style {
    if focused {
        Style::new().fg(Color::White)
    } else {
        Style::new().fg(Color::Gray)
    }
}

#[inline]
pub(super) fn filter_title(base: &str, filter: &str, filter_active: bool, focused: bool) -> String {
    if filter_active && focused {
        format!(" {base} /{filter}_ ")
    } else if !filter.is_empty() {
        format!(" {base} /{filter} ")
    } else {
        format!(" {base} ")
    }
}

#[inline]
pub(super) fn lang_icon(lang: Option<&str>) -> &'static str {
    match lang {
        Some("Rust") => "\u{e7a8} ",
        Some("Python") => "\u{e73c} ",
        Some("TypeScript") => "\u{e628} ",
        Some("JavaScript") => "\u{e74e} ",
        Some("Go") => "\u{e627} ",
        Some("Ruby") => "\u{e739} ",
        Some("Java") => "\u{e738} ",
        Some("Kotlin") => "\u{e634} ",
        Some("Swift") => "\u{e755} ",
        Some("C") => "\u{e61e} ",
        Some("C++") => "\u{e61d} ",
        Some("C#") => "\u{f031b} ",
        Some("Dockerfile") => "\u{e650} ",
        Some("Shell" | "Bash") => "\u{f489} ",
        Some("HTML") => "\u{e736} ",
        Some("CSS") => "\u{e749} ",
        Some("Lua") => "\u{e620} ",
        Some("Haskell") => "\u{e777} ",
        Some("Scala") => "\u{e737} ",
        Some("PHP") => "\u{e73d} ",
        Some("Elixir") => "\u{e62d} ",
        _ => ICON_REPO,
    }
}

#[inline]
pub(super) const fn review_icon(
    status: Option<&crate::types::ReviewStatus>,
) -> (&'static str, Color) {
    use crate::types::ReviewStatus;
    match status {
        Some(ReviewStatus::Approved) => ("\u{f012c}", Color::Green),
        Some(ReviewStatus::ChangesRequested) => ("\u{eb43}", Color::Red),
        Some(ReviewStatus::Pending) => ("\u{f444}", Color::DarkGray),
        Some(ReviewStatus::Unknown) | None => ("·", Color::DarkGray),
    }
}

#[inline]
pub(super) fn pr_state_icon(draft: bool, state: crate::types::PrState) -> &'static str {
    use crate::types::PrState;
    if draft {
        "\u{ebdb} "
    } else if state == PrState::Closed {
        "\u{f4dc} "
    } else {
        "\u{f407} "
    }
}

#[inline]
pub(super) fn truncate(s: &str, max: usize) -> String {
    if s.width() <= max {
        return s.to_string();
    }
    let mut out = String::new();
    let mut w = 0;
    for c in s.chars() {
        let cw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
        if w + cw + 1 > max {
            break;
        }
        out.push(c);
        w += cw;
    }
    out.push('…');
    out
}

#[inline]
pub(super) fn relative_time(iso: &str) -> String {
    use jiff::Timestamp;
    let Ok(ts) = iso.parse::<Timestamp>() else {
        return iso.get(..10).unwrap_or("?").to_string();
    };
    let secs = Timestamp::now().duration_since(ts).as_secs();
    match secs {
        0..=59 => "now".to_string(),
        60..=3_599 => format!("{}m", secs / 60),
        3_600..=86_399 => format!("{}h", secs / 3_600),
        86_400..=604_799 => format!("{}d", secs / 86_400),
        604_800..=2_419_199 => format!("{}w", secs / 604_800),
        2_419_200..=31_535_999 => format!("{}mo", secs / 2_592_000),
        _ => format!("{}y", secs / 31_536_000),
    }
}

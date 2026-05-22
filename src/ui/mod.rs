#![allow(clippy::redundant_pub_crate)]
mod markdown;
mod overlays;
mod panels;
mod status;

use crate::{
    app::App,
    types::{Column, RepoView, ReposView},
};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Scrollbar, ScrollbarOrientation, ScrollbarState},
};
use unicode_width::UnicodeWidthStr;

// Nerd Font glyphs — inline variants include a trailing space for separation from text
pub(super) const ICON_USER: &str = "\u{f007} ";
pub(super) const ICON_ORG: &str = "\u{f0af} ";
pub(super) const ICON_REPO: &str = "\u{e702} ";
pub(super) const ICON_CLOCK: &str = "\u{f017}";
pub(super) const ICON_CLOCK_UPDATED: &str = "\u{f520}";

// Bare glyphs (no trailing space) for strip/vertical layouts
pub(super) const ICON_USER_GLYPH: &str = "\u{f007}";
pub(super) const ICON_ORG_GLYPH: &str = "\u{f0af}";
pub(super) const ICON_REPO_GLYPH: &str = "\u{e702}";

// Repo list column header icons
pub(super) const ICON_STAR: &str = "\u{f005}";
pub(super) const ICON_FORK: &str = "\u{f126}";
pub(super) const ICON_BUG: &str = "\u{f41b}";
pub(super) const ICON_LOCK: &str = "\u{f023}";
pub(super) const ICON_ARCHIVE: &str = "\u{f187}";

// PR list column header icons
pub(super) const ICON_PR_HEADER: &str = "\u{f0f6}";
pub(super) const ICON_COMMENT: &str = "\u{f086}";
pub(super) const ICON_CHECKLIST: &str = "\u{f046}";

// CI check status icons
pub(super) const ICON_CHECK_PASS: &str = "\u{f058}";
pub(super) const ICON_CHECK_FAIL: &str = "\u{f0159}";
pub(super) const ICON_CHECK_PENDING: &str = "\u{e641}";

// PR state icons (trailing space included — layout-sensitive)
pub(super) const ICON_PR_DRAFT: &str = "\u{ebdb} ";
pub(super) const ICON_PR_CLOSED: &str = "\u{f4dc} ";
pub(super) const ICON_PR_OPEN: &str = "\u{f407} ";

// Issue state (open; closed reuses ICON_PR_CLOSED glyph)
pub(super) const ICON_ISSUE_OPEN: &str = "\u{f444} ";

// Review status icons
pub(super) const ICON_REVIEW_APPROVED: &str = "\u{f012c}";
pub(super) const ICON_REVIEW_CHANGES: &str = "\u{eb43}";
pub(super) const ICON_REVIEW_PENDING: &str = "\u{f444}";

// Middle dot — used for "none" / unknown states
pub(super) const ICON_DOT: &str = "\u{b7}";

// Language icons (trailing space for inline use before repo names)
pub(super) const LANG_RUST: &str = "\u{e7a8} ";
pub(super) const LANG_PYTHON: &str = "\u{e73c} ";
pub(super) const LANG_TYPESCRIPT: &str = "\u{e628} ";
pub(super) const LANG_JAVASCRIPT: &str = "\u{e74e} ";
pub(super) const LANG_GO: &str = "\u{e627} ";
pub(super) const LANG_RUBY: &str = "\u{e739} ";
pub(super) const LANG_JAVA: &str = "\u{e738} ";
pub(super) const LANG_KOTLIN: &str = "\u{e634} ";
pub(super) const LANG_SWIFT: &str = "\u{e755} ";
pub(super) const LANG_C: &str = "\u{e61e} ";
pub(super) const LANG_CPP: &str = "\u{e61d} ";
pub(super) const LANG_CSHARP: &str = "\u{f031b} ";
pub(super) const LANG_DOCKERFILE: &str = "\u{e650} ";
pub(super) const LANG_SHELL: &str = "\u{f489} ";
pub(super) const LANG_HTML: &str = "\u{e736} ";
pub(super) const LANG_CSS: &str = "\u{e749} ";
pub(super) const LANG_LUA: &str = "\u{e620} ";
pub(super) const LANG_HASKELL: &str = "\u{e777} ";
pub(super) const LANG_SCALA: &str = "\u{e737} ";
pub(super) const LANG_PHP: &str = "\u{e73d} ";
pub(super) const LANG_ELIXIR: &str = "\u{e62d} ";

pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();
    app.terminal_height = area.height;

    let chunks = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(area);

    let main_area = chunks[0];
    let status_area = chunks[1];

    if matches!(app.focus, Column::Repo | Column::Detail) && app.repos_view == ReposView::PrList {
        let cols = Layout::horizontal([
            Constraint::Length(4),
            Constraint::Fill(4),
            Constraint::Fill(3),
        ])
        .split(main_area);
        panels::draw_sources_strip(f, app, cols[0]);
        panels::draw_source_prs(f, app, cols[1]);
        panels::draw_pr_detail(f, app, cols[2]);
    } else if matches!(app.focus, Column::Repo | Column::Detail) {
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
        match app.repos_view {
            ReposView::RepoList => {
                panels::draw_repos(f, app, cols[1]);
                panels::draw_prs(f, app, cols[2]);
            }
            ReposView::PrList => {
                panels::draw_source_prs(f, app, cols[1]);
                panels::draw_pr_detail(f, app, cols[2]);
            }
        }
    }
    status::draw_status(f, app, status_area);

    if app.show_help {
        overlays::draw_help(f, app, area);
    }
    if app.show_dependabot_menu {
        overlays::draw_dependabot_menu(f, area);
    }
    if app.repo_ctx.diff_view.is_some() {
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
        Some("Rust") => LANG_RUST,
        Some("Python") => LANG_PYTHON,
        Some("TypeScript") => LANG_TYPESCRIPT,
        Some("JavaScript") => LANG_JAVASCRIPT,
        Some("Go") => LANG_GO,
        Some("Ruby") => LANG_RUBY,
        Some("Java") => LANG_JAVA,
        Some("Kotlin") => LANG_KOTLIN,
        Some("Swift") => LANG_SWIFT,
        Some("C") => LANG_C,
        Some("C++") => LANG_CPP,
        Some("C#") => LANG_CSHARP,
        Some("Dockerfile") => LANG_DOCKERFILE,
        Some("Shell" | "Bash") => LANG_SHELL,
        Some("HTML") => LANG_HTML,
        Some("CSS") => LANG_CSS,
        Some("Lua") => LANG_LUA,
        Some("Haskell") => LANG_HASKELL,
        Some("Scala") => LANG_SCALA,
        Some("PHP") => LANG_PHP,
        Some("Elixir") => LANG_ELIXIR,
        _ => ICON_REPO,
    }
}

pub(super) trait StatusLike: Copy {
    fn icon(self) -> &'static str;
    fn color(self) -> Color;
}

impl StatusLike for crate::types::CheckStatus {
    fn icon(self) -> &'static str {
        use crate::types::CheckStatus;
        match self {
            CheckStatus::Passing => ICON_CHECK_PASS,
            CheckStatus::Failing => ICON_CHECK_FAIL,
            CheckStatus::Pending => ICON_CHECK_PENDING,
            CheckStatus::Unknown => ICON_DOT,
        }
    }

    fn color(self) -> Color {
        use crate::types::CheckStatus;
        match self {
            CheckStatus::Passing => Color::Green,
            CheckStatus::Failing => Color::Red,
            CheckStatus::Pending => Color::Yellow,
            CheckStatus::Unknown => Color::DarkGray,
        }
    }
}

impl StatusLike for crate::types::ReviewStatus {
    fn icon(self) -> &'static str {
        use crate::types::ReviewStatus;
        match self {
            ReviewStatus::Approved => ICON_REVIEW_APPROVED,
            ReviewStatus::ChangesRequested => ICON_REVIEW_CHANGES,
            ReviewStatus::Pending => ICON_REVIEW_PENDING,
            ReviewStatus::Unknown => ICON_DOT,
        }
    }

    fn color(self) -> Color {
        use crate::types::ReviewStatus;
        match self {
            ReviewStatus::Approved => Color::Green,
            ReviewStatus::ChangesRequested => Color::Red,
            ReviewStatus::Pending | ReviewStatus::Unknown => Color::DarkGray,
        }
    }
}

#[inline]
pub(super) fn review_icon(
    status: Option<&crate::types::ReviewStatus>,
    merge: Option<&crate::types::MergeableState>,
) -> (&'static str, Color) {
    use crate::types::{MergeableState, ReviewStatus};
    let Some(s) = status else {
        return (ICON_DOT, Color::DarkGray);
    };
    let color = match s {
        ReviewStatus::Approved => match merge {
            Some(MergeableState::Dirty | MergeableState::Blocked) => Color::Red,
            Some(MergeableState::Behind | MergeableState::Unstable | MergeableState::Unknown)
            | None => Color::Yellow,
            Some(MergeableState::Clean | MergeableState::HasHooks) => Color::Green,
        },
        _ => s.color(),
    };
    (s.icon(), color)
}

#[inline]
pub(super) fn pr_state_icon(draft: bool, state: crate::types::PrState) -> &'static str {
    use crate::types::PrState;
    if draft {
        ICON_PR_DRAFT
    } else if state == PrState::Closed {
        ICON_PR_CLOSED
    } else {
        ICON_PR_OPEN
    }
}

#[inline]
pub(super) fn truncate(s: &str, max: usize) -> String {
    if s.width() <= max {
        return s.to_string();
    }
    if max == 0 {
        return String::new();
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

#![allow(clippy::redundant_pub_crate)]

mod detail;
mod prs;
mod repos;
mod sources;

pub(super) use detail::draw_pr_detail;
pub(super) use prs::{draw_prs, draw_source_issues, draw_source_prs};
pub(super) use repos::{
    draw_issue_detail, draw_issues, draw_repo_frontpage, draw_repos, draw_repos_strip,
};
pub(super) use sources::{draw_sources, draw_sources_strip};

// Re-export ui/mod.rs items for sub-modules.
pub(crate) use super::{
    ICON_ARCHIVE, ICON_BUG, ICON_CHECKLIST, ICON_CLOCK, ICON_CLOCK_UPDATED, ICON_COMMENT, ICON_DOT,
    ICON_FORK, ICON_LOCK, ICON_ORG, ICON_ORG_GLYPH, ICON_PR_DRAFT, ICON_PR_HEADER, ICON_REPO_GLYPH,
    ICON_STAR, ICON_USER, ICON_USER_GLYPH, StatusLike, active_style, filter_title, inactive_style,
    item_style, lang_icon, panel_focus, pr_state_icon, relative_time, render_list_scrollbar,
    review_icon, truncate,
};

use crate::types::{Label, MergeableState, PR, RepoView, ReposView};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap},
};
use unicode_width::UnicodeWidthStr;

pub(crate) const SPACES: &str = match core::str::from_utf8(&[b' '; 256]) {
    Ok(s) => s,
    Err(_) => unreachable!(),
};

pub(crate) fn gap_span(n: usize) -> Span<'static> {
    Span::raw(&SPACES[..n.min(SPACES.len())])
}

pub(crate) fn panel_block(title: String, style: Style) -> Block<'static> {
    Block::default()
        .title(title)
        .title_style(style)
        .borders(Borders::ALL)
        .border_style(style)
}

pub(crate) fn list_highlight_style() -> Style {
    Style::new()
        .bg(Color::Rgb(50, 60, 80))
        .fg(Color::White)
        .add_modifier(Modifier::BOLD)
}

pub(crate) fn dim_italic(text: &'static str) -> Paragraph<'static> {
    Paragraph::new(text).style(
        Style::new()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::ITALIC),
    )
}

pub(crate) fn loading_placeholder() -> Paragraph<'static> {
    Paragraph::new("Loading…").style(Style::new().fg(Color::DarkGray))
}

pub(crate) fn draw_scrollable_body(
    f: &mut Frame,
    body: Option<&String>,
    scroll: u16,
    content_area: Rect,
    sb_area: Rect,
) {
    match body {
        None => f.render_widget(loading_placeholder(), content_area),
        Some(b) if b.is_empty() => f.render_widget(dim_italic("(no description)"), content_area),
        Some(b) => {
            let md = super::markdown::render(b);
            let total_lines = Paragraph::new(md.clone())
                .wrap(Wrap { trim: false })
                .line_count(content_area.width);
            f.render_widget(
                Paragraph::new(md)
                    .wrap(Wrap { trim: false })
                    .scroll((scroll, 0)),
                content_area,
            );
            if total_lines > content_area.height as usize {
                let mut sb = ScrollbarState::new(total_lines).position(scroll as usize);
                f.render_stateful_widget(
                    Scrollbar::new(ScrollbarOrientation::VerticalRight),
                    sb_area,
                    &mut sb,
                );
            }
        }
    }
}

fn hex_to_rgb(hex: &str) -> Color {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return Color::DarkGray;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(128);
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(128);
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(128);
    Color::Rgb(r, g, b)
}

fn label_text_color(r: u8, g: u8, b: u8) -> Color {
    let brightness = (r as u32 * 299 + g as u32 * 587 + b as u32 * 114) / 1000;
    if brightness > 128 {
        Color::Black
    } else {
        Color::White
    }
}

fn label_ends_wide(name: &str) -> bool {
    use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};
    let chars: Vec<char> = name.chars().collect();
    let Some(base) = (0..chars.len())
        .rev()
        .find(|&i| UnicodeWidthChar::width(chars[i]).unwrap_or(0) > 0)
    else {
        return false;
    };
    let cluster: String = chars[base..].iter().collect();
    cluster.width() > 1
}

pub(crate) fn label_pill_w(label: &Label) -> usize {
    // left-cap(1) + space(1) + name + trailing-space(0 or 1) + right-cap(1)
    3 + label.name.width() + usize::from(!label_ends_wide(&label.name))
}

pub(crate) fn label_pill_spans(label: &Label, cap_bg: Color) -> [Span<'static>; 3] {
    let bg = hex_to_rgb(&label.color);
    let (r, g, b) = match bg {
        Color::Rgb(r, g, b) => (r, g, b),
        _ => (128, 128, 128),
    };
    let fg = label_text_color(r, g, b);
    let trailing = if label_ends_wide(&label.name) {
        ""
    } else {
        " "
    };
    [
        Span::styled("\u{e0b6}", Style::new().fg(bg).bg(cap_bg)),
        Span::styled(
            format!(" {}{trailing}", label.name),
            Style::new().fg(fg).bg(bg),
        ),
        Span::styled("\u{e0b4}", Style::new().fg(bg).bg(cap_bg)),
    ]
}

pub(crate) fn pad_to_width(spans: Vec<Span<'static>>, cur_w: usize, width: usize) -> Line<'static> {
    let mut spans = spans;
    let pad = width.saturating_sub(cur_w);
    if pad > 0 {
        spans.push(gap_span(pad));
    }
    Line::from(spans)
}

pub(crate) fn wrap_label_lines(
    labels: &[Label],
    width: usize,
    cap_bg: Color,
) -> Vec<Line<'static>> {
    if labels.is_empty() {
        return vec![];
    }
    let mut lines: Vec<Line<'static>> = vec![];
    let mut cur_spans: Vec<Span<'static>> = vec![];
    let mut cur_w = 0usize;
    for lbl in labels {
        let pill_w = label_pill_w(lbl);
        let sep = usize::from(cur_w > 0);
        if cur_w + sep + pill_w > width && cur_w > 0 {
            lines.push(pad_to_width(std::mem::take(&mut cur_spans), cur_w, width));
            cur_w = 0;
        } else if sep > 0 {
            cur_spans.push(Span::raw(" "));
            cur_w += 1;
        }
        cur_w += pill_w;
        cur_spans.extend(label_pill_spans(lbl, cap_bg));
    }
    if !cur_spans.is_empty() {
        lines.push(pad_to_width(cur_spans, cur_w, width));
    }
    lines
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn view_tab_line(
    current: RepoView,
    focused: bool,
    show_prs: bool,
    show_issues: bool,
    pr_count: usize,
    pr_has_more: bool,
    issue_count: usize,
    issue_has_more: bool,
) -> Line<'static> {
    let sep = Span::raw("  ");
    let key_active = Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD);
    let key_dim = Style::new().fg(Color::DarkGray);
    let key_disabled = Style::new()
        .fg(Color::DarkGray)
        .add_modifier(Modifier::CROSSED_OUT);
    let label_active = Style::new().fg(Color::White).add_modifier(Modifier::BOLD);
    let label_dim = Style::new().fg(Color::DarkGray);
    let label_disabled = Style::new()
        .fg(Color::DarkGray)
        .add_modifier(Modifier::CROSSED_OUT);

    let tab = |key: &'static str, label: String, view: RepoView, enabled: bool| {
        if !enabled {
            vec![
                Span::styled(key, key_disabled),
                Span::styled(label, label_disabled),
            ]
        } else if view == current {
            let (ks, ls) = if focused {
                (key_active, label_active)
            } else {
                (
                    key_dim,
                    Style::new()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::BOLD),
                )
            };
            vec![Span::styled(key, ks), Span::styled(label, ls)]
        } else {
            vec![Span::styled(key, key_dim), Span::styled(label, label_dim)]
        }
    };

    let pr_label = if pr_count > 0 {
        let suffix = if pr_has_more { "+" } else { "" };
        format!("·prs ({pr_count}{suffix})")
    } else {
        "·prs".to_string()
    };
    let issue_label = if issue_count > 0 {
        let suffix = if issue_has_more { "+" } else { "" };
        format!("·issues ({issue_count}{suffix})")
    } else {
        "·issues".to_string()
    };

    let mut spans = vec![Span::raw(" ")];
    spans.extend(tab("f", "·page".to_string(), RepoView::Frontpage, true));
    spans.push(sep.clone());
    spans.extend(tab("p", pr_label, RepoView::Prs, show_prs));
    spans.push(sep.clone());
    spans.extend(tab("i", issue_label, RepoView::Issues, show_issues));
    spans.push(Span::raw(" "));
    Line::from(spans)
}

pub(crate) fn repos_tab_line(
    current: ReposView,
    pr_count: usize,
    pr_has_more: bool,
    issue_count: usize,
    issue_has_more: bool,
) -> Line<'static> {
    let key_active = Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD);
    let key_dim = Style::new().fg(Color::DarkGray);
    let label_active = Style::new().fg(Color::White).add_modifier(Modifier::BOLD);
    let label_dim = Style::new().fg(Color::DarkGray);

    let tab_style = |view: ReposView| {
        if view == current {
            (key_active, label_active)
        } else {
            (key_dim, label_dim)
        }
    };

    let pr_label = if pr_count > 0 {
        let suffix = if pr_has_more { "+" } else { "" };
        format!("·prs ({pr_count}{suffix})")
    } else {
        "·prs".to_string()
    };
    let issue_label = if issue_count > 0 {
        let suffix = if issue_has_more { "+" } else { "" };
        format!("·issues ({issue_count}{suffix})")
    } else {
        "·issues".to_string()
    };

    let (rk, rl) = tab_style(ReposView::RepoList);
    let (pk, pl) = tab_style(ReposView::PrList);
    let (ik, il) = tab_style(ReposView::IssueList);

    Line::from(vec![
        Span::raw(" "),
        Span::styled("r", rk),
        Span::styled("·repos", rl),
        Span::raw("  "),
        Span::styled("p", pk),
        Span::styled(pr_label, pl),
        Span::raw("  "),
        Span::styled("i", ik),
        Span::styled(issue_label, il),
        Span::raw(" "),
    ])
}

pub(crate) fn mergeable_state_span(state: Option<&MergeableState>) -> Option<Span<'static>> {
    match state {
        Some(MergeableState::Behind) => {
            Some(Span::styled("⟳ rebase  ", Style::new().fg(Color::Yellow)))
        }
        Some(MergeableState::Dirty) => {
            Some(Span::styled("✖ conflicts  ", Style::new().fg(Color::Red)))
        }
        _ => None,
    }
}

pub(crate) fn diff_stat_spans(pr: &PR) -> Option<(Span<'static>, Span<'static>)> {
    if pr.additions == 0 && pr.deletions == 0 {
        return None;
    }
    Some((
        Span::styled(
            format!("+{}", fmt_stat(pr.additions)),
            Style::new().fg(Color::Green),
        ),
        Span::styled(
            format!("-{}", fmt_stat(pr.deletions)),
            Style::new().fg(Color::Red),
        ),
    ))
}

pub(crate) fn render_markdown(s: &str) -> ratatui::text::Text<'static> {
    super::markdown::render(s)
}

pub(crate) fn draw_strip_vertical(
    f: &mut Frame,
    inner: Rect,
    icon: &str,
    icon_style: Style,
    name: &str,
    name_style: Style,
) {
    let mut lines: Vec<Line> = Vec::with_capacity(inner.height as usize);
    lines.push(Line::from(Span::styled(icon.to_string(), icon_style)));
    lines.extend(
        name.chars()
            .take(inner.height.saturating_sub(1) as usize)
            .map(|ch| Line::from(Span::styled(ch.to_string(), name_style))),
    );
    f.render_widget(Paragraph::new(lines), inner);
}

fn fmt_stat(n: u32) -> String {
    if n < 1_000 {
        n.to_string()
    } else if n < 10_000 {
        format!("{:.1}k", n as f32 / 1_000.0)
    } else {
        format!("{}k", n / 1_000)
    }
}

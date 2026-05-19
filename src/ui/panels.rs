use super::{
    ICON_ARCHIVE, ICON_BUG, ICON_CHECK_FAIL, ICON_CHECK_PASS, ICON_CHECK_PENDING, ICON_CHECKLIST,
    ICON_CLOCK, ICON_CLOCK_UPDATED, ICON_COMMENT, ICON_DOT, ICON_FORK, ICON_ISSUE_OPEN, ICON_LOCK,
    ICON_ORG, ICON_ORG_GLYPH, ICON_PR_CLOSED, ICON_PR_HEADER, ICON_REPO_GLYPH, ICON_STAR,
    ICON_USER, ICON_USER_GLYPH, active_style, filter_title, inactive_style, item_style, lang_icon,
    panel_focus, pr_state_icon, relative_time, render_list_scrollbar, review_icon, truncate,
};
use crate::{
    app::App,
    types::{
        CheckStatus, Column, DetailSection, LoadingKind, PrColumn, PrState, RepoColumn, RepoView,
        Source, Visibility,
    },
};
use unicode_width::UnicodeWidthStr;

const SPACES: &str = match core::str::from_utf8(&[b' '; 256]) {
    Ok(s) => s,
    Err(_) => unreachable!(),
};

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{
        Block, Borders, List, ListItem, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
        Wrap,
    },
};

fn panel_block(title: String, style: Style) -> Block<'static> {
    Block::default()
        .title(title)
        .title_style(style)
        .borders(Borders::ALL)
        .border_style(style)
}

fn list_highlight_style() -> Style {
    Style::new()
        .bg(Color::Rgb(50, 60, 80))
        .fg(Color::White)
        .add_modifier(Modifier::BOLD)
}

fn dim_italic(text: &'static str) -> Paragraph<'static> {
    Paragraph::new(text).style(
        Style::new()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::ITALIC),
    )
}

fn loading_placeholder() -> Paragraph<'static> {
    Paragraph::new("Loading…").style(Style::new().fg(Color::DarkGray))
}

fn gap_span(n: usize) -> Span<'static> {
    Span::raw(&SPACES[..n.min(SPACES.len())])
}

/// Proportional bar spans filling `width` columns, colored by check status counts.
fn checks_bar_spans(runs: &[crate::types::CheckRun], width: usize) -> Vec<Span<'static>> {
    if runs.is_empty() || width == 0 {
        return vec![];
    }
    let mut counts = [0usize; 4]; // [failing, pending, unknown, passing]
    for r in runs {
        match r.status {
            CheckStatus::Failing => counts[0] += 1,
            CheckStatus::Pending => counts[1] += 1,
            CheckStatus::Unknown => counts[2] += 1,
            CheckStatus::Passing => counts[3] += 1,
        }
    }
    let total = runs.len();
    let colors = [Color::Red, Color::Yellow, Color::DarkGray, Color::Green];
    let mut spans = Vec::new();
    let mut used = 0usize;
    for (i, (&count, &color)) in counts.iter().zip(colors.iter()).enumerate() {
        if count == 0 {
            continue;
        }
        let cols = if i == 3 {
            // last bucket: fill remainder to avoid rounding gaps
            width.saturating_sub(used)
        } else {
            (count * width / total).max(1)
        };
        let cols = cols.min(width.saturating_sub(used));
        if cols == 0 {
            continue;
        }
        spans.push(Span::styled("█".repeat(cols), Style::new().fg(color)));
        used += cols;
        if used >= width {
            break;
        }
    }
    spans
}

fn draw_scrollable_body(
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

/// Wrap labels into lines that fit within `width`, returning styled `Line`s.
fn wrap_label_lines(labels: &[String], width: usize, color: Color) -> Vec<Line<'static>> {
    if labels.is_empty() {
        return vec![];
    }
    let mut lines: Vec<Line<'static>> = vec![];
    let mut cur_spans: Vec<Span<'static>> = vec![];
    let mut cur_w = 0usize;
    for lbl in labels {
        let tag = format!("[{lbl}]");
        let sep = usize::from(cur_w > 0);
        if cur_w + sep + tag.len() > width && cur_w > 0 {
            lines.push(Line::from(std::mem::take(&mut cur_spans)));
            cur_w = 0;
        } else if sep > 0 {
            cur_spans.push(Span::raw(" "));
            cur_w += 1;
        }
        cur_w += tag.len();
        cur_spans.push(Span::styled(tag, Style::new().fg(color)));
    }
    if !cur_spans.is_empty() {
        lines.push(Line::from(cur_spans));
    }
    lines
}

pub(super) fn draw_sources(f: &mut Frame, app: &mut App, area: ratatui::layout::Rect) {
    let focused = app.focus == Column::Sources;
    let border_style = panel_focus(focused);

    let loading_suffix = match &app.loading {
        Some(LoadingKind::Sources) => " ⟳",
        _ => "",
    };

    let base = format!("Sources{loading_suffix}");
    let title = filter_title(&base, &app.source_filter, app.filter_active, focused);

    let block = panel_block(title, border_style);

    let visible = app.visible_sources();
    let items: Vec<ListItem> = visible
        .iter()
        .map(|s| {
            let (icon, color) = match s {
                Source::User(_) => (ICON_USER, Color::Cyan),
                Source::Org(_) => (ICON_ORG, Color::Yellow),
            };
            let label = s.display();
            let style = if focused {
                Style::new().fg(color)
            } else {
                Style::new().fg(Color::DarkGray)
            };
            let line = Line::from(vec![
                Span::styled(icon, style.add_modifier(Modifier::BOLD)),
                Span::styled(label, style),
            ]);
            ListItem::new(line)
        })
        .collect();

    if items.is_empty() && app.loading.is_none() {
        let inner = block.inner(area);
        f.render_widget(block, area);
        f.render_widget(
            Paragraph::new("Run: gh auth login").style(Style::new().fg(Color::DarkGray)),
            inner,
        );
        return;
    }

    let total = visible.len();
    let list = List::new(items)
        .block(block)
        .highlight_style(list_highlight_style())
        .highlight_symbol("▶ ");

    f.render_stateful_widget(list, area, &mut app.source_state);
    render_list_scrollbar(
        f,
        area,
        total,
        area.height.saturating_sub(2),
        app.source_state.offset(),
    );
}

pub(super) fn draw_repos(f: &mut Frame, app: &mut App, area: ratatui::layout::Rect) {
    let focused = app.focus == Column::Repos;
    let border_style = panel_focus(focused);

    let loading_suffix = match &app.loading {
        Some(LoadingKind::Repos) => " ⟳",
        _ => "",
    };

    let sort_label = app.repo_sort_key.label();
    let repo_count_suffix = if app.filter_active || !app.repo_filter.is_empty() {
        let visible = app.visible_repos().len();
        let total = app.repos.len();
        format!("  {visible}/{total}")
    } else {
        String::new()
    };
    let base = app.selected_source().map_or_else(
        || format!("Repo List  {sort_label}{loading_suffix}{repo_count_suffix}"),
        |s| {
            format!(
                "Repo List  {}  {sort_label}{loading_suffix}{repo_count_suffix}",
                s.display()
            )
        },
    );
    let title = filter_title(&base, &app.repo_filter, app.filter_active, focused);

    let block = panel_block(title, border_style);

    let cols_cfg: &[RepoColumn] = if focused {
        &app.config.ui.repo_columns
    } else {
        &[]
    };
    // column widths: Stars/Forks/Issues = 4 digits max, Visibility = 1, LastPush/Created = 3
    let col_w = |c: &RepoColumn| match c {
        RepoColumn::Stars | RepoColumn::Forks | RepoColumn::Issues => 4usize,
        RepoColumn::Visibility => 1,
        RepoColumn::LastPush | RepoColumn::Created => 3,
    };
    let col_width: usize = cols_cfg.iter().map(col_w).sum::<usize>()
        + if cols_cfg.is_empty() {
            0
        } else {
            cols_cfg.len() - 1
        };

    // 4 = 2 borders + 2 highlight symbol
    let inner_width = area.width.saturating_sub(4) as usize;

    let inner = block.inner(area);
    f.render_widget(block, area);

    let [header_area, body_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(inner);

    // render column header when focused and columns configured
    if focused && !cols_cfg.is_empty() {
        let header_style = Style::new()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::BOLD);
        let mut header_parts = String::new();
        for (i, col) in cols_cfg.iter().enumerate() {
            if i > 0 {
                header_parts.push(' ');
            }
            let w = col_w(col);
            let sym = match col {
                RepoColumn::Stars => ICON_STAR,
                RepoColumn::Forks => ICON_FORK,
                RepoColumn::Issues => ICON_BUG,
                RepoColumn::Visibility => ICON_LOCK,
                RepoColumn::LastPush => ICON_CLOCK,
                RepoColumn::Created => ICON_CLOCK_UPDATED,
            };
            header_parts.push_str(&format!("{sym:>w$}"));
        }
        let gap = inner_width.saturating_sub(col_width + 2);
        let header_line = Line::from(vec![
            Span::raw("  "),
            gap_span(gap),
            Span::styled(header_parts, header_style),
        ]);
        f.render_widget(Paragraph::new(header_line), header_area);
    }

    let items: Vec<ListItem> = app
        .visible_repos()
        .into_iter()
        .map(|repo| {
            let style = item_style(focused);
            let dim = Style::new().fg(Color::DarkGray);

            let mut right_spans: Vec<Span> = Vec::with_capacity(cols_cfg.len() * 2);
            for (i, col) in cols_cfg.iter().enumerate() {
                if i > 0 {
                    right_spans.push(Span::raw(" "));
                }
                match col {
                    RepoColumn::Stars => right_spans.push(Span::styled(
                        format!("{:>w$}", fmt_count(repo.stars), w = col_w(col)),
                        Style::new().fg(Color::Yellow),
                    )),
                    RepoColumn::Forks => right_spans.push(Span::styled(
                        format!("{:>w$}", fmt_count(repo.forks), w = col_w(col)),
                        dim,
                    )),
                    RepoColumn::Issues => right_spans.push(Span::styled(
                        format!("{:>w$}", fmt_count(repo.issues), w = col_w(col)),
                        Style::new().fg(Color::Cyan),
                    )),
                    RepoColumn::Visibility => {
                        let (sym, color) = match repo.visibility {
                            Visibility::Private => ("P", Color::Yellow),
                            Visibility::Internal => ("I", Color::Cyan),
                            Visibility::Public => (ICON_DOT, Color::DarkGray),
                        };
                        right_spans.push(Span::styled(sym, Style::new().fg(color)));
                    }
                    RepoColumn::LastPush => {
                        let age = repo
                            .pushed_at
                            .as_deref()
                            .map(super::relative_time)
                            .unwrap_or_else(|| "—".into());
                        right_spans.push(Span::styled(format!("{:>3}", age), dim));
                    }
                    RepoColumn::Created => {
                        let age = repo
                            .created_at
                            .as_deref()
                            .map(super::relative_time)
                            .unwrap_or_else(|| "—".into());
                        right_spans.push(Span::styled(format!("{:>3}", age), dim));
                    }
                }
            }

            let icon = lang_icon(repo.language.as_deref());
            let icon_style = if focused {
                Style::new().fg(Color::Magenta)
            } else {
                Style::new().fg(Color::DarkGray)
            };
            let archive_badge_w = if repo.archived { 2 } else { 0 };
            let name_budget = inner_width.saturating_sub(
                icon.width() + archive_badge_w + if col_width > 0 { col_width + 1 } else { 0 },
            );
            let name_text = truncate(&repo.name, name_budget);
            let gap = inner_width.saturating_sub(
                icon.width()
                    + name_text.width()
                    + archive_badge_w
                    + col_width
                    + usize::from(col_width > 0),
            );

            let name_style = if repo.archived {
                Style::new().fg(Color::DarkGray)
            } else {
                style
            };
            let mut spans = vec![
                Span::styled(icon, icon_style),
                Span::styled(name_text, name_style),
            ];
            if repo.archived {
                spans.push(Span::styled(
                    format!(" {ICON_ARCHIVE}"),
                    Style::new().fg(Color::DarkGray),
                ));
            }
            spans.push(gap_span(gap));
            spans.extend(right_spans);
            ListItem::new(Line::from(spans))
        })
        .collect();

    let total = items.len();
    if total == 0 && !app.repo_filter.is_empty() && app.loading.is_none() {
        f.render_widget(
            Paragraph::new(format!("no results for \"{}\"", app.repo_filter))
                .style(Style::new().fg(Color::DarkGray)),
            body_area,
        );
        return;
    }
    let list = List::new(items)
        .highlight_style(list_highlight_style())
        .highlight_symbol("▶ ");

    f.render_stateful_widget(list, body_area, &mut app.repo_state);
    render_list_scrollbar(
        f,
        area,
        total,
        area.height.saturating_sub(3),
        app.repo_state.offset(),
    );
}

pub(super) fn draw_prs(f: &mut Frame, app: &mut App, area: ratatui::layout::Rect) {
    let focused = app.focus == Column::Repo;
    let border_style = panel_focus(focused);

    let loading_suffix = match &app.loading {
        Some(LoadingKind::Prs) => " ⟳".to_string(),
        Some(LoadingKind::Action(a)) => format!(" {a}…"),
        _ => String::new(),
    };

    let sort_label = app.sort_key.label();
    let owner_repo = app.selected_owner_repo();
    let pr_count_suffix = if app.filter_active || !app.pr_filter.is_empty() {
        format!("  {}/{}", app.prs.len(), app.prs_raw.len())
    } else {
        String::new()
    };
    let base = if let Some((ref owner, ref repo)) = owner_repo {
        format!("{owner}/{repo}  {sort_label}{loading_suffix}{pr_count_suffix}")
    } else {
        format!("Pull Requests  {sort_label}{loading_suffix}{pr_count_suffix}")
    };
    let title = filter_title(&base, &app.pr_filter, app.filter_active, focused);

    let block = panel_block(title, border_style).title_bottom(view_tab_line(
        RepoView::Prs,
        app.selected_repo_has_issues(),
        app.prs_raw.len(),
        app.prs_has_more,
        app.issues.len(),
        app.issues_has_more,
    ));

    // 4 = 2 borders + 2 highlight-symbol ("▶ ")
    let inner_width = area.width.saturating_sub(4) as usize;

    let age_col = 4usize;
    let status_col = 1 + 1 + 2; // 1sp + rv + 2sp
    let show_diff = app.config.ui.pr_columns.contains(&PrColumn::DiffStats);
    let show_age = app.config.ui.pr_columns.contains(&PrColumn::Age);
    let show_updated = app.config.ui.pr_columns.contains(&PrColumn::UpdatedAt);
    let show_comments = app.config.ui.pr_columns.contains(&PrColumn::Comments);
    let show_check_summary = app.config.ui.pr_columns.contains(&PrColumn::CheckSummary);
    // "+9.9k -9.9k" = 11 chars max; time cols provide trailing separator
    let diff_col: usize = 11;
    // each time col: 2 sep + age_col value
    let time_col_w = 2 + age_col;
    // comment col: 2 sep + up to 3 digits
    let comment_col_w = 2 + 3;
    // check summary col: 2 sep + 1 icon
    let check_summary_col_w = 2 + 1;
    let right_col_width = if show_comments { comment_col_w } else { 0 }
        + if show_check_summary {
            check_summary_col_w
        } else {
            0
        }
        + if show_diff { diff_col } else { 0 }
        + status_col
        + if show_updated { time_col_w } else { 0 }
        + if show_age { time_col_w } else { 0 };

    let items: Vec<ListItem> = app
        .prs
        .iter()
        .map(|pr| {
            let dimmed = pr.draft || pr.state == PrState::Closed;
            let base_style = if dimmed {
                Style::new().fg(Color::DarkGray)
            } else {
                item_style(focused)
            };
            let meta_style = Style::new().fg(Color::DarkGray);

            let (rv_sym, rv_col) = review_icon(app.review_statuses.get(&pr.number));

            let number_str = format!("#{} ", pr.number);
            let age_str = if show_age {
                let age = relative_time(&pr.created_at);
                format!("  {:>agecol$}", age, agecol = age_col)
            } else {
                String::new()
            };
            let updated_str = if show_updated {
                let upd = relative_time(&pr.updated_at);
                format!("  {:>agecol$}", upd, agecol = age_col)
            } else {
                String::new()
            };
            let num_w = number_str.width();
            // line1 left: "#N by @author"
            let by_str = format!("by @{}", pr.author);
            let left_w = num_w + by_str.width();
            let gap = inner_width.saturating_sub(left_w + right_col_width);

            let mut line1_spans = vec![
                Span::styled(number_str, Style::new().add_modifier(Modifier::BOLD)),
                Span::styled("by ", meta_style),
                Span::styled(
                    format!("@{}", pr.author),
                    meta_style.add_modifier(Modifier::BOLD),
                ),
                gap_span(gap),
            ];
            if show_comments {
                let n = pr.comments;
                let count_str = if n > 999 {
                    "99+".to_string()
                } else {
                    format!("{n:>3}")
                };
                line1_spans.push(Span::styled(
                    format!("  {:>width$}", count_str, width = comment_col_w - 2),
                    meta_style,
                ));
            }
            if show_check_summary {
                let (icon, color) = match app.check_summary_cache.get(&pr.number) {
                    Some(CheckStatus::Passing) => (ICON_CHECK_PASS, Color::Green),
                    Some(CheckStatus::Failing) => (ICON_CHECK_FAIL, Color::Red),
                    Some(CheckStatus::Pending) => (ICON_CHECK_PENDING, Color::Yellow),
                    Some(CheckStatus::Unknown) | None => (ICON_DOT, Color::DarkGray),
                };
                line1_spans.push(Span::raw("  "));
                line1_spans.push(Span::styled(icon, Style::new().fg(color)));
            }
            line1_spans.extend([
                Span::raw(" "),
                Span::styled(rv_sym, Style::new().fg(rv_col)),
                Span::raw("  "),
            ]);
            if show_diff {
                if pr.additions == 0 && pr.deletions == 0 {
                    line1_spans.push(Span::raw(format!("{:width$}", "", width = diff_col)));
                } else {
                    let add_str = format!("+{}", fmt_stat(pr.additions));
                    let del_str = format!("-{}", fmt_stat(pr.deletions));
                    let content_w = add_str.len() + 1 + del_str.len();
                    let pad = diff_col.saturating_sub(content_w);
                    line1_spans.push(Span::styled(add_str, Style::new().fg(Color::Green)));
                    line1_spans.push(Span::raw(" "));
                    line1_spans.push(Span::styled(del_str, Style::new().fg(Color::Red)));
                    line1_spans.push(Span::raw(" ".repeat(pad)));
                }
            }
            line1_spans.extend([
                Span::styled(updated_str, meta_style),
                Span::styled(age_str, meta_style),
            ]);
            let line1 = Line::from(line1_spans);

            // line2: "  [state] [merge_warn] [title] [labels]"
            let state_icon = pr_state_icon(pr.draft, pr.state);
            let state_col = if pr.draft {
                Color::DarkGray
            } else if pr.state == PrState::Closed {
                Color::Red
            } else {
                Color::Green
            };
            let merge_state_w: usize = match app.mergeable_states.get(&pr.number) {
                Some(crate::types::MergeableState::Behind) => "⟳ rebase  ".width(),
                Some(crate::types::MergeableState::Dirty) => "✖ conflicts  ".width(),
                _ => 0,
            };
            let label_section_w: usize = if pr.labels.is_empty() {
                0
            } else {
                2 + pr.labels.iter().map(|l| l.len() + 2).sum::<usize>()
                    + pr.labels.len().saturating_sub(1)
            };
            // prefix: "  "(2) + state_icon(2) + " "(1) = 5
            let title2_budget = inner_width.saturating_sub(5 + merge_state_w + label_section_w);
            let title2_text = truncate(&pr.title, title2_budget);

            let mut meta_spans: Vec<Span> = vec![
                Span::raw("  "),
                Span::styled(state_icon, Style::new().fg(state_col)),
                Span::raw(" "),
            ];
            match app.mergeable_states.get(&pr.number) {
                Some(crate::types::MergeableState::Behind) => {
                    meta_spans.push(Span::styled("⟳ rebase  ", Style::new().fg(Color::Yellow)))
                }
                Some(crate::types::MergeableState::Dirty) => {
                    meta_spans.push(Span::styled("✖ conflicts  ", Style::new().fg(Color::Red)))
                }
                _ => {}
            }
            meta_spans.push(Span::styled(title2_text, base_style));
            if !pr.labels.is_empty() {
                meta_spans.push(Span::raw("  "));
                for (i, label) in pr.labels.iter().enumerate() {
                    if i > 0 {
                        meta_spans.push(Span::raw(" "));
                    }
                    meta_spans.push(Span::styled(
                        format!("[{label}]"),
                        Style::new().fg(Color::Green),
                    ));
                }
            }
            let line2 = Line::from(meta_spans);
            ListItem::new(Text::from(vec![line1, line2]))
        })
        .collect();

    let inner = block.inner(area);
    f.render_widget(block, area);

    let [header_area, body_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(inner);

    let header_style = Style::new()
        .fg(Color::DarkGray)
        .add_modifier(Modifier::BOLD);
    let status_header = format!(" {ICON_PR_HEADER}  ");
    let comment_header = if show_comments {
        format!("{:>width$}", ICON_COMMENT, width = comment_col_w)
    } else {
        String::new()
    };
    let check_summary_header = if show_check_summary {
        format!("  {ICON_CHECKLIST}")
    } else {
        String::new()
    };
    let diff_header = if show_diff {
        format!("{:<width$}", "±", width = diff_col)
    } else {
        String::new()
    };
    let age_header = if show_age {
        format!("{:>width$}", ICON_CLOCK, width = time_col_w)
    } else {
        String::new()
    };
    let updated_header = if show_updated {
        format!("{:>width$}", ICON_CLOCK_UPDATED, width = time_col_w)
    } else {
        String::new()
    };
    let right_header = format!(
        "{comment_header}{check_summary_header}{status_header}{diff_header}{updated_header}{age_header}"
    );
    let gap = inner_width.saturating_sub(right_col_width);
    let header_line = Line::from(vec![
        Span::raw("  "),
        gap_span(gap),
        Span::styled(right_header, header_style),
    ]);
    f.render_widget(Paragraph::new(header_line), header_area);

    if items.is_empty() && app.loading.is_none() {
        let msg = if !app.pr_filter.is_empty() {
            format!("no results for \"{}\"", app.pr_filter)
        } else if owner_repo.is_some() {
            "No open pull requests".to_string()
        } else {
            "Select a repo".to_string()
        };
        f.render_widget(
            Paragraph::new(msg).style(Style::new().fg(Color::DarkGray)),
            body_area,
        );
        return;
    }

    let total = items.len();
    let list = List::new(items)
        .highlight_style(list_highlight_style())
        .highlight_symbol("▶ ");

    f.render_stateful_widget(list, body_area, &mut app.pr_state);
    render_list_scrollbar(f, area, total * 2, body_area.height, app.pr_state.offset());
}

fn draw_strip_vertical(
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

pub(super) fn draw_sources_strip(f: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(inactive_style());
    let inner = block.inner(area);
    f.render_widget(block, area);
    let Some(source) = app.selected_source() else {
        return;
    };
    let (icon, color) = match source {
        Source::User(_) => (ICON_USER_GLYPH, Color::Cyan),
        Source::Org(_) => (ICON_ORG_GLYPH, Color::Yellow),
    };
    let style = Style::new().fg(color).add_modifier(Modifier::BOLD);
    draw_strip_vertical(f, inner, icon, style, &source.display(), style);
}

pub(super) fn draw_repos_strip(f: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(inactive_style());
    let inner = block.inner(area);
    f.render_widget(block, area);
    let Some(name) = app.selected_repo() else {
        return;
    };
    draw_strip_vertical(
        f,
        inner,
        ICON_REPO_GLYPH,
        Style::new().fg(Color::DarkGray),
        name,
        Style::new().fg(Color::White).add_modifier(Modifier::BOLD),
    );
}

pub(super) fn draw_pr_detail(f: &mut Frame, app: &mut App, area: ratatui::layout::Rect) {
    let in_detail = app.focus == Column::Detail;
    let pr = app.selected_pr();
    let title = pr.map_or_else(|| " Detail ".to_string(), |pr| format!(" #{} ", pr.number));
    let outer_style = if in_detail {
        active_style()
    } else {
        inactive_style()
    };
    let block = Block::default()
        .title(title)
        .title_style(outer_style)
        .borders(Borders::ALL)
        .border_style(outer_style);

    let inner = block.inner(area);
    f.render_widget(block, area);

    if pr.is_none() {
        f.render_widget(
            Paragraph::new("Select a PR").style(Style::new().fg(Color::DarkGray)),
            inner,
        );
        return;
    }

    let pr = pr.unwrap();
    let has_meta = pr.additions > 0
        || pr.deletions > 0
        || !pr.labels.is_empty()
        || !pr.head_ref.is_empty()
        || !pr.requested_reviewers.is_empty()
        || matches!(
            app.mergeable_states.get(&pr.number),
            Some(crate::types::MergeableState::Behind | crate::types::MergeableState::Dirty)
        );
    let title_lines = u16::try_from(
        Paragraph::new(pr.title.as_str())
            .wrap(Wrap { trim: false })
            .line_count(inner.width),
    )
    .unwrap_or(1)
    .max(1);
    let header_height = title_lines + u16::from(has_meta);

    let body_focusable = app.pr_body_focusable();
    let checks_focusable = app.checks_focusable();
    let body_constraint = if body_focusable {
        Constraint::Min(3)
    } else {
        Constraint::Length(3)
    };
    let checks_constraint = if checks_focusable {
        let h = if body_focusable {
            (inner.height * 2 / 5).max(4)
        } else {
            0
        };
        if body_focusable {
            Constraint::Length(h)
        } else {
            Constraint::Min(4)
        }
    } else {
        Constraint::Length(3)
    };

    let bar_runs = app.check_runs.as_deref().unwrap_or(&[]);
    let has_bar = !bar_runs.is_empty();

    let [header_area, body_area, checks_area] = Layout::vertical([
        Constraint::Length(header_height),
        body_constraint,
        checks_constraint,
    ])
    .areas(inner);

    // Title + labels header
    let title_line = Line::from(Span::styled(
        pr.title.clone(),
        Style::new().fg(Color::White).add_modifier(Modifier::BOLD),
    ));
    let mut header_lines = vec![title_line];
    let mut meta_line_spans: Vec<Span> = vec![];
    let add_str = fmt_stat(pr.additions);
    let del_str = fmt_stat(pr.deletions);
    if pr.additions > 0 || pr.deletions > 0 {
        meta_line_spans.push(Span::styled(
            format!("+{add_str}"),
            Style::new().fg(Color::Green),
        ));
        meta_line_spans.push(Span::raw(" "));
        meta_line_spans.push(Span::styled(
            format!("-{del_str}"),
            Style::new().fg(Color::Red),
        ));
        meta_line_spans.push(Span::raw("  "));
    }
    match app.mergeable_states.get(&pr.number) {
        Some(crate::types::MergeableState::Behind) => {
            meta_line_spans.push(Span::styled("⟳ rebase  ", Style::new().fg(Color::Yellow)))
        }
        Some(crate::types::MergeableState::Dirty) => {
            meta_line_spans.push(Span::styled("✖ conflicts  ", Style::new().fg(Color::Red)))
        }
        _ => {}
    }
    if !pr.head_ref.is_empty() {
        if !meta_line_spans.is_empty() {
            meta_line_spans.push(Span::raw("  "));
        }
        meta_line_spans.push(Span::styled(
            format!("{} → {}", pr.head_ref, pr.base_ref),
            Style::new().fg(Color::DarkGray),
        ));
    }
    if !pr.requested_reviewers.is_empty() {
        if !meta_line_spans.is_empty() {
            meta_line_spans.push(Span::raw("  "));
        }
        meta_line_spans.push(Span::styled(
            format!("👁 {}", pr.requested_reviewers.join(", ")),
            Style::new().fg(Color::Magenta),
        ));
    }
    for (i, lbl) in pr.labels.iter().enumerate() {
        if i == 0 && !meta_line_spans.is_empty() {
            meta_line_spans.push(Span::raw("  "));
        } else if i > 0 {
            meta_line_spans.push(Span::raw(" "));
        }
        meta_line_spans.push(Span::styled(
            format!("[{lbl}]"),
            Style::new().fg(Color::Cyan),
        ));
    }
    if !meta_line_spans.is_empty() {
        header_lines.push(Line::from(meta_line_spans));
    }
    f.render_widget(
        Paragraph::new(Text::from(header_lines)).wrap(Wrap { trim: false }),
        header_area,
    );

    let body_active = in_detail && app.detail_section == DetailSection::Body;
    let checks_active = in_detail && app.detail_section == DetailSection::Checks;

    // Body section
    let body_style = if body_active {
        active_style()
    } else {
        inactive_style()
    };
    let body_block = Block::default()
        .title(if body_active && checks_focusable {
            " Description  Tab→ "
        } else {
            " Description "
        })
        .title_style(body_style)
        .borders(Borders::ALL)
        .border_style(body_style);
    let body_inner = body_block.inner(body_area);
    f.render_widget(body_block, body_area);

    draw_scrollable_body(
        f,
        app.pr_body.as_ref(),
        app.pr_body_scroll,
        body_inner,
        body_area,
    );

    // Checks section
    let checks_style = if checks_active {
        active_style()
    } else {
        inactive_style()
    };
    let checks_block = Block::default()
        .title(if checks_active && body_focusable {
            " Checks  Tab→ "
        } else {
            " Checks "
        })
        .title_style(checks_style)
        .borders(Borders::ALL)
        .border_style(checks_style);
    let checks_inner = checks_block.inner(checks_area);
    f.render_widget(checks_block, checks_area);

    let [bar_area, list_area] =
        Layout::vertical([Constraint::Length(u16::from(has_bar)), Constraint::Min(0)])
            .areas(checks_inner);
    if has_bar {
        let spans = checks_bar_spans(bar_runs, bar_area.width as usize);
        f.render_widget(Paragraph::new(Line::from(spans)), bar_area);
    }

    match &app.check_runs {
        None => {
            f.render_widget(loading_placeholder(), list_area);
        }
        Some(runs) if runs.is_empty() => {
            f.render_widget(dim_italic("(no checks)"), list_area);
        }
        Some(runs) => {
            let items: Vec<ListItem> = runs
                .iter()
                .map(|run| {
                    let (icon, color) = match run.status {
                        CheckStatus::Passing => (ICON_CHECK_PASS, Color::Green),
                        CheckStatus::Failing => (ICON_CHECK_FAIL, Color::Red),
                        CheckStatus::Pending => (ICON_CHECK_PENDING, Color::Yellow),
                        CheckStatus::Unknown => (ICON_DOT, Color::DarkGray),
                    };
                    Line::from(vec![
                        Span::styled(format!("{icon} "), Style::new().fg(color)),
                        Span::styled(run.name.clone(), Style::new().fg(Color::White)),
                    ])
                    .into()
                })
                .collect();
            let list = List::new(items)
                .highlight_style(list_highlight_style())
                .highlight_symbol("▶ ");
            f.render_stateful_widget(list, list_area, &mut app.check_runs_state);
            if runs.len() > list_area.height as usize {
                let mut sb =
                    ScrollbarState::new(runs.len()).position(app.check_runs_state.offset());
                f.render_stateful_widget(
                    Scrollbar::new(ScrollbarOrientation::VerticalRight),
                    checks_area,
                    &mut sb,
                );
            }
        }
    }
}

fn view_tab_line(
    current: RepoView,
    show_issues: bool,
    pr_count: usize,
    pr_has_more: bool,
    issue_count: usize,
    issue_has_more: bool,
) -> Line<'static> {
    let sep = Span::raw("  ");
    let key_active = Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD);
    let key_dim = Style::new().fg(Color::DarkGray);
    let label_active = Style::new().fg(Color::White).add_modifier(Modifier::BOLD);
    let label_dim = Style::new().fg(Color::DarkGray);

    let tab = |key: &'static str, label: String, view: RepoView| {
        if view == current {
            vec![
                Span::styled(key, key_active),
                Span::styled(label, label_active),
            ]
        } else {
            vec![Span::styled(key, key_dim), Span::styled(label, label_dim)]
        }
    };

    let pr_label = if pr_count > 0 {
        let suffix = if pr_has_more { "+" } else { "" };
        format!("·prs ({}{})", pr_count, suffix)
    } else {
        "·prs".to_string()
    };
    let issue_label = if issue_count > 0 {
        let suffix = if issue_has_more { "+" } else { "" };
        format!("·issues ({}{})", issue_count, suffix)
    } else {
        "·issues".to_string()
    };

    let mut spans = vec![Span::raw(" ")];
    spans.extend(tab("f", "·page".to_string(), RepoView::Frontpage));
    spans.push(sep.clone());
    spans.extend(tab("p", pr_label, RepoView::Prs));
    if show_issues {
        spans.push(sep.clone());
        spans.extend(tab("i", issue_label, RepoView::Issues));
    }
    spans.push(Span::raw(" "));
    Line::from(spans)
}

pub(super) fn draw_repo_frontpage(f: &mut Frame, app: &mut App, area: ratatui::layout::Rect) {
    let repo_name = app.selected_repo().map(str::to_string).unwrap_or_default();
    let scroll = app.repo_frontpage_scroll;
    let border_style = active_style();

    let block = panel_block(format!(" {repo_name} "), border_style).title_bottom(view_tab_line(
        RepoView::Frontpage,
        app.selected_repo_has_issues(),
        app.prs_raw.len(),
        app.prs_has_more,
        app.issues.len(),
        app.issues_has_more,
    ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let frontpage = app.repo_frontpage.clone();
    match frontpage {
        None => {
            f.render_widget(loading_placeholder(), inner);
        }
        Some((description, readme)) if description.is_empty() && readme.is_empty() => {
            f.render_widget(dim_italic("(no readme)"), inner);
        }
        Some((description, readme)) => {
            let mut lines: Vec<Line> = Vec::new();
            if !description.is_empty() {
                lines.push(Line::from(Span::styled(
                    description,
                    Style::new().fg(Color::Yellow),
                )));
                lines.push(Line::raw(""));
            }
            if readme.is_empty() {
                lines.push(Line::from(Span::styled(
                    "(no readme)",
                    Style::new()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::ITALIC),
                )));
            } else {
                let md = super::markdown::render(&readme);
                lines.extend(md.lines);
            }
            let content = Text::from(lines);
            let total_lines = Paragraph::new(content.clone())
                .wrap(Wrap { trim: false })
                .line_count(inner.width);
            f.render_widget(
                Paragraph::new(content)
                    .wrap(Wrap { trim: false })
                    .scroll((scroll, 0)),
                inner,
            );
            if total_lines > inner.height as usize {
                let mut sb = ScrollbarState::new(total_lines).position(scroll as usize);
                f.render_stateful_widget(
                    Scrollbar::new(ScrollbarOrientation::VerticalRight),
                    area,
                    &mut sb,
                );
            }
        }
    }
}

pub(super) fn draw_issues(f: &mut Frame, app: &mut App, area: ratatui::layout::Rect) {
    let focused = app.focus == Column::Repo;
    let border_style = panel_focus(focused);

    let loading_suffix = if matches!(app.loading, Some(LoadingKind::Issues)) {
        " ⟳"
    } else {
        ""
    };
    let owner_repo = app.selected_owner_repo();
    let base = if let Some((ref owner, ref repo)) = owner_repo {
        format!(" {owner}/{repo}{loading_suffix} ")
    } else {
        format!(" Issues{loading_suffix} ")
    };

    let block = panel_block(base, border_style).title_bottom(view_tab_line(
        RepoView::Issues,
        true,
        app.prs_raw.len(),
        app.prs_has_more,
        app.issues.len(),
        app.issues_has_more,
    ));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let [header_area, body_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(inner);
    let header_style = Style::new()
        .fg(Color::DarkGray)
        .add_modifier(Modifier::BOLD);
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw("  "),
            Span::styled("#    Title", header_style),
        ])),
        header_area,
    );

    let inner_width = area.width.saturating_sub(4) as usize;
    let age_col = 4usize;
    let author_col = app
        .issues
        .iter()
        .map(|i| i.author.len())
        .max()
        .unwrap_or(6)
        .clamp(6, 20);

    let items: Vec<ListItem> = app
        .issues
        .iter()
        .map(|issue| {
            let number_str = format!("#{} ", issue.number);
            let num_w = number_str.len();
            let age = relative_time(&issue.created_at);
            let author_str = format!("@{:<acol$}", issue.author, acol = author_col);
            let age_str = format!("  {ICON_CLOCK} {:>agecol$}", age, agecol = age_col);
            // 2 sep + 1 icon (display) + 1 space + age_col
            let author_age_w = author_str.width() + 2 + 1 + 1 + age_col;
            let title_budget = inner_width.saturating_sub(num_w + author_age_w + 1);
            let title_text = truncate(&issue.title, title_budget);
            let title_w = title_text.len();
            let gap = inner_width.saturating_sub(num_w + title_w + author_age_w);

            let line1 = Line::from(vec![
                Span::styled(number_str, Style::new().add_modifier(Modifier::BOLD)),
                Span::styled(title_text, item_style(focused)),
                gap_span(gap),
                Span::styled(
                    author_str,
                    Style::new()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(age_str, Style::new().fg(Color::DarkGray)),
            ]);

            let (state_icon, state_color) = if issue.state == "closed" {
                (ICON_PR_CLOSED, Color::Red)
            } else {
                (ICON_ISSUE_OPEN, Color::Green)
            };
            let icon_line = Line::from(vec![
                Span::raw("  "),
                Span::styled(state_icon, Style::new().fg(state_color)),
            ]);
            let mut text_lines = vec![line1, icon_line];
            text_lines.extend(wrap_label_lines(&issue.labels, inner_width, Color::Green));
            ListItem::new(Text::from(text_lines))
        })
        .collect();

    if items.is_empty() && !matches!(app.loading, Some(LoadingKind::Issues)) {
        let msg = if owner_repo.is_some() {
            "No open issues"
        } else {
            "Select a repo"
        };
        f.render_widget(
            Paragraph::new(msg).style(Style::new().fg(Color::DarkGray)),
            body_area,
        );
        return;
    }

    let total = items.len();
    let list = List::new(items)
        .highlight_style(list_highlight_style())
        .highlight_symbol("▶ ");
    f.render_stateful_widget(list, body_area, &mut app.issue_state);
    render_list_scrollbar(
        f,
        area,
        total * 2,
        body_area.height,
        app.issue_state.offset(),
    );
}

pub(super) fn draw_issue_detail(f: &mut Frame, app: &mut App, area: ratatui::layout::Rect) {
    let in_detail = app.focus == Column::Detail;
    let issue_number = app.selected_issue().map(|i| i.number);
    let issue_title = app.selected_issue().map(|i| i.title.clone());
    let issue_labels = app
        .selected_issue()
        .map(|i| i.labels.clone())
        .unwrap_or_default();
    let title = issue_number.map_or_else(|| " Issues ".to_string(), |n| format!(" Issue #{n} "));
    let outer_style = if in_detail {
        active_style()
    } else {
        inactive_style()
    };

    let block = panel_block(title, outer_style);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let Some(title_text) = issue_title else {
        f.render_widget(
            Paragraph::new("Select an issue").style(Style::new().fg(Color::DarkGray)),
            inner,
        );
        return;
    };

    let title_lines = u16::try_from(
        Paragraph::new(title_text.as_str())
            .wrap(Wrap { trim: false })
            .line_count(inner.width),
    )
    .unwrap_or(1)
    .max(1);

    let label_lines = wrap_label_lines(&issue_labels, inner.width as usize, Color::Cyan);
    let label_line_count = u16::try_from(label_lines.len()).unwrap_or(0);
    let header_height = title_lines + label_line_count;
    let [header_area, body_area] =
        Layout::vertical([Constraint::Length(header_height), Constraint::Min(0)]).areas(inner);

    let mut header_lines = vec![Line::from(Span::styled(
        title_text,
        Style::new().fg(Color::White).add_modifier(Modifier::BOLD),
    ))];
    header_lines.extend(label_lines);
    f.render_widget(
        Paragraph::new(Text::from(header_lines)).wrap(Wrap { trim: false }),
        header_area,
    );

    draw_scrollable_body(
        f,
        app.issue_body.as_ref(),
        app.issue_body_scroll,
        body_area,
        area,
    );
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

// Always 4 chars wide.
fn fmt_count(n: u32) -> String {
    match n {
        0..10_000 => format!("{n:>4}"),
        10_000..100_000 => format!("{:>3}k", n / 1_000),
        100_000..1_000_000 => format!("{}k", n / 1_000),
        1_000_000.. => format!("{:>3}m", n / 1_000_000),
    }
}

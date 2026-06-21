use super::{
    ICON_ARCHIVE, ICON_BUG, ICON_CLOCK, ICON_CLOCK_UPDATED, ICON_DOT, ICON_FORK, ICON_LOCK,
    ICON_REPO_GLYPH, ICON_STAR, active_style, dim_italic, draw_scrollable_body,
    draw_strip_vertical, filter_title, gap_span, inactive_style, item_style, lang_icon,
    list_highlight_style, loading_placeholder, panel_block, panel_focus, relative_time,
    render_list_scrollbar, render_markdown, repos_tab_line, truncate, view_tab_line,
    wrap_label_lines,
};
use crate::{
    app::App,
    types::{Column, LoadingKind, RepoColumn, RepoView, ReposView, Visibility},
};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{
        Block, Borders, Clear, List, ListItem, Paragraph, Scrollbar, ScrollbarOrientation,
        ScrollbarState, Wrap,
    },
};
use std::fmt::Write as _;
use unicode_width::UnicodeWidthStr;

// Always 4 chars wide.
fn fmt_count(n: u32) -> String {
    match n {
        0..10_000 => format!("{n:>4}"),
        10_000..100_000 => format!("{:>3}k", n / 1_000),
        100_000..1_000_000 => format!("{}k", n / 1_000),
        1_000_000.. => format!("{:>3}m", n / 1_000_000),
    }
}

pub(crate) fn draw_repos(f: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focus == Column::Repos;
    let border_style = panel_focus(focused);

    let loading_suffix = match &app.loading {
        Some(LoadingKind::Repos) => " ⟳",
        _ => "",
    };

    let sort_label = app.repo_sort_key.label();
    let repo_count_suffix = if app.filter_active || !app.source_ctx.repo_filter.is_empty() {
        let visible = app.visible_repos().len();
        let total = app.source_ctx.repos.len();
        format!("  {visible}/{total}")
    } else {
        String::new()
    };
    let base = app.selected_source().map_or_else(String::new, |s| {
        format!(
            " {}  {sort_label}{loading_suffix}{repo_count_suffix}",
            s.display()
        )
    });
    let title = filter_title(
        &base,
        &app.source_ctx.repo_filter,
        app.filter_active,
        focused,
    );

    let block = panel_block(title, border_style).title_bottom(repos_tab_line(
        ReposView::RepoList,
        app.source_ctx.source_prs.len(),
        app.source_ctx.source_prs_pagination.has_more,
        app.source_ctx.source_issues.len(),
        app.source_ctx.source_issues_pagination.has_more,
    ));

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
            let _ = write!(header_parts, "{sym:>w$}");
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
                            .map_or_else(|| "—".into(), relative_time);
                        right_spans.push(Span::styled(format!("{age:>3}"), dim));
                    }
                    RepoColumn::Created => {
                        let age = repo
                            .created_at
                            .as_deref()
                            .map_or_else(|| "—".into(), relative_time);
                        right_spans.push(Span::styled(format!("{age:>3}"), dim));
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
    if total == 0 && !app.source_ctx.repo_filter.is_empty() && app.loading.is_none() {
        f.render_widget(
            Paragraph::new(format!("no results for \"{}\"", app.source_ctx.repo_filter))
                .style(Style::new().fg(Color::DarkGray)),
            body_area,
        );
        return;
    }
    let list = List::new(items)
        .highlight_style(list_highlight_style())
        .highlight_symbol("▶ ");

    f.render_stateful_widget(list, body_area, &mut app.source_ctx.repo_state);
    render_list_scrollbar(
        f,
        area,
        total,
        area.height.saturating_sub(3),
        app.source_ctx.repo_state.offset(),
    );
}

pub(crate) fn draw_repos_strip(f: &mut Frame, app: &App, area: Rect) {
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

pub(crate) fn draw_repo_frontpage(f: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focus == Column::Repo;
    let repo_name = app.selected_repo().map(str::to_string).unwrap_or_default();
    let scroll = app.repo_ctx.repo_frontpage_scroll;
    let border_style = panel_focus(focused);

    let block = panel_block(format!(" {repo_name} "), border_style).title_bottom(view_tab_line(
        RepoView::Frontpage,
        focused,
        app.selected_repo_has_prs(),
        app.selected_repo_has_issues(),
        app.repo_ctx.prs_raw.len(),
        app.repo_ctx.prs_pagination.has_more,
        app.repo_ctx.issues.len(),
        app.repo_ctx.issues_pagination.has_more,
    ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let frontpage = app.repo_ctx.repo_frontpage.clone();
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
                let md = render_markdown(&readme);
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

pub(crate) fn draw_issues(f: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focus == Column::Repo;
    let border_style = panel_focus(focused);

    let loading_suffix = if matches!(app.loading, Some(LoadingKind::Issues)) {
        " ⟳"
    } else {
        ""
    };
    let owner_repo = app.selected_owner_repo();
    let base = if let Some(ref rid) = owner_repo {
        format!(" {rid}{loading_suffix} ")
    } else {
        format!(" Issues{loading_suffix} ")
    };

    let block = panel_block(base, border_style).title_bottom(view_tab_line(
        RepoView::Issues,
        focused,
        app.selected_repo_has_prs(),
        true,
        app.repo_ctx.prs_raw.len(),
        app.repo_ctx.prs_pagination.has_more,
        app.repo_ctx.issues.len(),
        app.repo_ctx.issues_pagination.has_more,
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
        .repo_ctx
        .issues
        .iter()
        .map(|i| i.author.len())
        .max()
        .unwrap_or(6)
        .clamp(6, 20);

    let selected_idx = app.repo_ctx.issue_state.selected();
    let items: Vec<ListItem> = app
        .repo_ctx
        .issues
        .iter()
        .enumerate()
        .map(|(i, issue)| {
            let is_selected = selected_idx == Some(i);
            let hl = if is_selected {
                list_highlight_style()
            } else {
                Style::default()
            };
            let cap_bg = if is_selected {
                Color::Rgb(50, 60, 80)
            } else {
                Color::Reset
            };
            let meta_fg = if is_selected {
                Color::Gray
            } else {
                Color::DarkGray
            };

            let number_str = format!("#{} ", issue.number);
            let num_w = number_str.len();
            let age = relative_time(&issue.created_at);
            let author_str = format!("@{:<acol$}", issue.author, acol = author_col);
            let age_str = format!("  {ICON_CLOCK} {age:>age_col$}");
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
                    Style::new().fg(meta_fg).add_modifier(Modifier::BOLD),
                ),
                Span::styled(age_str, Style::new().fg(meta_fg)),
            ])
            .style(hl);

            let mut text_lines = vec![line1];
            text_lines.extend(
                wrap_label_lines(&issue.labels, inner_width, cap_bg)
                    .into_iter()
                    .map(|line| line.style(hl)),
            );
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
    let list = List::new(items).highlight_symbol("▶ ");
    f.render_widget(Clear, body_area);
    f.render_stateful_widget(list, body_area, &mut app.repo_ctx.issue_state);
    render_list_scrollbar(
        f,
        area,
        total,
        body_area.height,
        app.repo_ctx.issue_state.offset(),
    );
}

pub(crate) fn draw_issue_detail(f: &mut Frame, app: &mut App, area: Rect) {
    let in_detail = app.focus == Column::Detail;
    let issue_number = app.selected_issue().map(|i| i.number);
    let issue_title = app.selected_issue().map(|i| i.title.clone());
    let issue_labels = app
        .selected_issue()
        .map(|i| i.labels.clone())
        .unwrap_or_default();
    let issue_repo = app
        .selected_issue()
        .map(|i| i.repo.clone())
        .filter(|r| !r.is_empty());
    let title = match (issue_number, issue_repo) {
        (Some(n), Some(repo)) => format!(" {repo} #{n} "),
        (Some(n), None) => format!(" Issue #{n} "),
        _ => " Issues ".to_string(),
    };
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

    let label_lines = wrap_label_lines(&issue_labels, inner.width as usize, Color::Reset);
    let label_line_count = u16::try_from(label_lines.len()).unwrap_or(0);
    let header_height = title_lines + label_line_count;
    let [header_area, body_area] =
        Layout::vertical([Constraint::Length(header_height), Constraint::Min(0)]).areas(inner);

    let mut header_lines = vec![Line::from(Span::styled(
        title_text,
        Style::new().fg(Color::White).add_modifier(Modifier::BOLD),
    ))];
    header_lines.extend(label_lines);
    f.render_widget(Clear, inner);
    f.render_widget(
        Paragraph::new(Text::from(header_lines)).wrap(Wrap { trim: false }),
        header_area,
    );

    draw_scrollable_body(
        f,
        app.repo_ctx.issue_body.as_ref(),
        app.repo_ctx.issue_body_scroll,
        body_area,
        area,
    );
}

use super::{
    ICON_CHECKLIST, ICON_CLOCK, ICON_CLOCK_UPDATED, ICON_COMMENT, ICON_DOT, ICON_ISSUE_OPEN,
    ICON_PR_CLOSED, ICON_PR_HEADER, StatusLike, diff_stat_spans, dim_italic, filter_title,
    gap_span, item_style, list_highlight_style, loading_placeholder, mergeable_state_span,
    panel_block, panel_focus, pr_state_icon, relative_time, render_list_scrollbar, repos_tab_line,
    review_icon, truncate, view_tab_line, wrap_label_lines,
};
use crate::{
    app::App,
    types::{Column, LoadingKind, PR, PrColumn, PrState, RepoView, ReposView},
};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{List, ListItem, Paragraph},
};
use unicode_width::UnicodeWidthStr;

pub(crate) struct PrListCols {
    pub(super) show_comments: bool,
    pub(super) show_check_summary: bool,
    pub(super) show_diff: bool,
    pub(super) show_updated: bool,
    pub(super) show_age: bool,
    pub(super) age_col: usize,
    pub(super) diff_col: usize,
    pub(super) time_col_w: usize,
    pub(super) comment_col_w: usize,
    pub(super) right_col_width: usize,
}

impl PrListCols {
    pub(super) fn new(config: &crate::config::Config) -> Self {
        let age_col: usize = 4;
        let diff_col: usize = 11;
        let time_col_w = 2 + age_col;
        let comment_col_w: usize = 5; // 2 sep + 3 digits
        let show_comments = config.ui.pr_columns.contains(&PrColumn::Comments);
        let show_check_summary = config.ui.pr_columns.contains(&PrColumn::CheckSummary);
        let show_diff = config.ui.pr_columns.contains(&PrColumn::DiffStats);
        let show_updated = config.ui.pr_columns.contains(&PrColumn::UpdatedAt);
        let show_age = config.ui.pr_columns.contains(&PrColumn::Age);
        let right_col_width = if show_comments { comment_col_w } else { 0 }
            + if show_check_summary { 3 } else { 0 } // 2 sep + 1 icon
            + if show_diff { diff_col } else { 0 }
            + 4 // 1sp + rv + 2sp
            + if show_updated { time_col_w } else { 0 }
            + if show_age { time_col_w } else { 0 };
        Self {
            show_comments,
            show_check_summary,
            show_diff,
            show_updated,
            show_age,
            age_col,
            diff_col,
            time_col_w,
            comment_col_w,
            right_col_width,
        }
    }
}

fn pr_list_header(cols: &PrListCols, inner_width: usize) -> Line<'static> {
    let header_style = Style::new()
        .fg(Color::DarkGray)
        .add_modifier(Modifier::BOLD);
    let comment_header = if cols.show_comments {
        format!("{ICON_COMMENT:>width$}", width = cols.comment_col_w)
    } else {
        String::new()
    };
    let check_header = if cols.show_check_summary {
        format!("  {ICON_CHECKLIST}")
    } else {
        String::new()
    };
    let status_header = format!(" {ICON_PR_HEADER}  ");
    let diff_header = if cols.show_diff {
        format!("{:<width$}", "±", width = cols.diff_col)
    } else {
        String::new()
    };
    let updated_header = if cols.show_updated {
        format!("{ICON_CLOCK_UPDATED:>width$}", width = cols.time_col_w)
    } else {
        String::new()
    };
    let age_header = if cols.show_age {
        format!("{ICON_CLOCK:>width$}", width = cols.time_col_w)
    } else {
        String::new()
    };
    let right = format!(
        "{comment_header}{check_header}{status_header}{diff_header}{updated_header}{age_header}"
    );
    let gap = inner_width.saturating_sub(cols.right_col_width);
    Line::from(vec![
        Span::raw("  "),
        gap_span(gap),
        Span::styled(right, header_style),
    ])
}

fn pr_state_color(pr: &PR) -> Color {
    if pr.draft {
        Color::DarkGray
    } else if pr.state == PrState::Closed {
        Color::Red
    } else {
        Color::Green
    }
}

/// Build list items shared by both the repo PR list and the source PR list.
///
/// `repo_override`: `Some(repo_name)` for the per-repo list (all PRs share the same repo,
/// prefix shows `#N`); `None` for the source-level list (each PR carries its own `pr.repo`,
/// prefix shows `repo #N`).
fn build_pr_list_items(
    app: &App,
    prs: &[&PR],
    owner: &str,
    repo_override: Option<&str>,
    inner_width: usize,
    focused: bool,
    cols: &PrListCols,
) -> Vec<ListItem<'static>> {
    use crate::types::RepoId;
    prs.iter()
        .map(|pr| {
            let dimmed = pr.is_dimmed();
            let repo_name = repo_override.unwrap_or(&pr.repo);
            let pr_id = RepoId::new(owner, repo_name).pr(pr.number);
            let base_style = if dimmed {
                Style::new().fg(Color::DarkGray)
            } else {
                item_style(focused)
            };
            let meta_style = Style::new().fg(Color::DarkGray);

            let rv_status = app
                .review_cache
                .get(&pr_id.repo.key())
                .and_then(|m| m.get(&pr.number));
            let (rv_sym, rv_col) =
                review_icon(rv_status, app.repo_ctx.mergeable_states.get(&pr_id));

            let left_budget = inner_width.saturating_sub(cols.right_col_width);
            let mut line1_spans: Vec<Span> = if repo_override.is_some() {
                let number_str = format!("#{} ", pr.number);
                let num_w = number_str.width();
                let by_str = truncate(
                    &format!("by @{}", pr.author),
                    left_budget.saturating_sub(num_w),
                );
                let gap = left_budget.saturating_sub(num_w + by_str.width());
                vec![
                    Span::styled(number_str, Style::new().add_modifier(Modifier::BOLD)),
                    Span::styled(by_str, meta_style),
                    gap_span(gap),
                ]
            } else {
                let repo_num = truncate(&format!("{} #{}", pr.repo, pr.number), left_budget);
                let remaining = left_budget.saturating_sub(repo_num.width());
                let by_str = if remaining > 1 {
                    truncate(&format!("by @{}", pr.author), remaining - 1)
                } else {
                    String::new()
                };
                let gap = left_budget.saturating_sub(
                    repo_num.width() + by_str.width() + usize::from(!by_str.is_empty()),
                );
                vec![
                    Span::styled(repo_num, base_style.add_modifier(Modifier::BOLD)),
                    Span::raw(if by_str.is_empty() { "" } else { " " }),
                    Span::styled(by_str, meta_style),
                    gap_span(gap),
                ]
            };

            if cols.show_comments {
                let n = pr.comments;
                let count_str = if n > 999 {
                    "99+".to_string()
                } else {
                    format!("{n:>3}")
                };
                line1_spans.push(Span::styled(
                    format!("  {:>width$}", count_str, width = cols.comment_col_w - 2),
                    meta_style,
                ));
            }
            if cols.show_check_summary {
                let (icon, color) = app
                    .repo_ctx
                    .check_summary_cache
                    .get(&pr_id)
                    .map_or((ICON_DOT, Color::DarkGray), |s| (s.icon(), s.color()));
                line1_spans.push(Span::raw("  "));
                line1_spans.push(Span::styled(icon, Style::new().fg(color)));
            }
            line1_spans.extend([
                Span::raw(" "),
                Span::styled(rv_sym, Style::new().fg(rv_col)),
                Span::raw("  "),
            ]);
            if cols.show_diff {
                match diff_stat_spans(pr) {
                    None => line1_spans.push(gap_span(cols.diff_col)),
                    Some((add_span, del_span)) => {
                        let content_w = add_span.width() + 1 + del_span.width();
                        let pad = cols.diff_col.saturating_sub(content_w);
                        line1_spans.extend([add_span, Span::raw(" "), del_span, gap_span(pad)]);
                    }
                }
            }
            if cols.show_updated {
                let upd = relative_time(&pr.updated_at);
                line1_spans.push(Span::styled(
                    format!("  {upd:>width$}", width = cols.age_col),
                    meta_style,
                ));
            }
            if cols.show_age {
                let age = relative_time(&pr.created_at);
                line1_spans.push(Span::styled(
                    format!("  {age:>width$}", width = cols.age_col),
                    meta_style,
                ));
            }
            let line1 = Line::from(line1_spans);

            let state_icon = pr_state_icon(pr.draft, pr.state);
            let state_col = pr_state_color(pr);
            let merge_state_w = mergeable_state_span(app.repo_ctx.mergeable_states.get(&pr_id))
                .as_ref()
                .map_or(0, Span::width);
            let title2_budget = inner_width.saturating_sub(5 + merge_state_w);
            let title2_text = truncate(&pr.title, title2_budget);
            let mut line2_spans: Vec<Span> = vec![
                Span::raw("  "),
                Span::styled(state_icon, Style::new().fg(state_col)),
                Span::raw(" "),
            ];
            if let Some(s) = mergeable_state_span(app.repo_ctx.mergeable_states.get(&pr_id)) {
                line2_spans.push(s);
            }
            line2_spans.push(Span::styled(title2_text, base_style));
            ListItem::new(Text::from(vec![line1, Line::from(line2_spans)]))
        })
        .collect()
}

pub(crate) fn draw_prs(f: &mut Frame, app: &mut App, area: Rect) {
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
        format!(
            "  {}/{}",
            app.repo_ctx.prs.len(),
            app.repo_ctx.prs_raw.len()
        )
    } else {
        String::new()
    };
    let base = if let Some(ref rid) = owner_repo {
        format!("{rid}  {sort_label}{loading_suffix}{pr_count_suffix}")
    } else {
        format!("Pull Requests  {sort_label}{loading_suffix}{pr_count_suffix}")
    };
    let title = filter_title(&base, &app.pr_filter, app.filter_active, focused);
    let block = panel_block(title, border_style).title_bottom(view_tab_line(
        RepoView::Prs,
        focused,
        app.selected_repo_has_prs(),
        app.selected_repo_has_issues(),
        app.repo_ctx.prs_raw.len(),
        app.repo_ctx.prs_pagination.has_more,
        app.repo_ctx.issues.len(),
        app.repo_ctx.issues_pagination.has_more,
    ));

    let inner_width = area.width.saturating_sub(4) as usize;
    let cols = PrListCols::new(&app.config);
    let owner = app.selected_source_owner().unwrap_or_default();
    let repo = owner_repo.as_ref().map(|r| r.repo.as_str()).unwrap_or("");
    let prs_ref: Vec<&PR> = app.repo_ctx.prs.iter().collect();
    let items = build_pr_list_items(
        app,
        &prs_ref,
        &owner,
        Some(repo),
        inner_width,
        focused,
        &cols,
    );

    let inner = block.inner(area);
    f.render_widget(block, area);
    let [header_area, body_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(inner);
    f.render_widget(
        Paragraph::new(pr_list_header(&cols, inner_width)),
        header_area,
    );

    if items.is_empty() && app.loading.is_none() {
        let msg = if !app.pr_filter.is_empty() {
            format!("no results for \"{}\"", app.pr_filter)
        } else if owner_repo.is_some() {
            if app.selected_repo_has_prs() {
                "No open pull requests".to_string()
            } else {
                "Pull requests are disabled for this repository".to_string()
            }
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
    f.render_stateful_widget(list, body_area, &mut app.repo_ctx.pr_state);
    render_list_scrollbar(
        f,
        area,
        total * 2,
        body_area.height,
        app.repo_ctx.pr_state.offset(),
    );
}

pub(crate) fn draw_source_prs(f: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focus == Column::Repos;
    let border_style = panel_focus(focused);

    let loading_suffix = match &app.loading {
        Some(LoadingKind::Prs) => " ⟳".to_string(),
        Some(LoadingKind::Action(a)) => format!(" {a}…"),
        _ => String::new(),
    };
    let source_name = app
        .selected_source()
        .map(|s| s.display().clone())
        .unwrap_or_default();
    let pr_count_suffix = if app.filter_active || !app.source_ctx.source_pr_filter.is_empty() {
        format!(
            "  {}/{}",
            app.visible_source_prs().len(),
            app.source_ctx.source_prs.len()
        )
    } else {
        String::new()
    };
    let base = format!(" {source_name}{loading_suffix}{pr_count_suffix} ");
    let title = filter_title(
        &base,
        &app.source_ctx.source_pr_filter,
        app.filter_active && app.focus == Column::Repos,
        focused,
    );
    let block = panel_block(title, border_style).title_bottom(repos_tab_line(
        ReposView::PrList,
        app.source_ctx.source_prs.len(),
        app.source_ctx.source_prs_pagination.has_more,
        app.source_ctx.source_issues.len(),
        app.source_ctx.source_issues_pagination.has_more,
    ));

    let inner_width = area.width.saturating_sub(4) as usize;
    let cols = PrListCols::new(&app.config);
    let owner = app.selected_source_owner().unwrap_or_default();
    let visible_prs = app.visible_source_prs();
    let items = build_pr_list_items(app, &visible_prs, &owner, None, inner_width, focused, &cols);

    let inner = block.inner(area);
    f.render_widget(block, area);
    let [header_area, body_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(inner);
    f.render_widget(
        Paragraph::new(pr_list_header(&cols, inner_width)),
        header_area,
    );

    if visible_prs.is_empty() {
        if app.loading.is_some() {
            f.render_widget(loading_placeholder(), body_area);
        } else if !app.source_ctx.source_pr_filter.is_empty() {
            f.render_widget(dim_italic("no results"), body_area);
        } else {
            f.render_widget(dim_italic("(no open PRs)"), body_area);
        }
        return;
    }

    let total = items.len();
    let list = List::new(items)
        .highlight_style(list_highlight_style())
        .highlight_symbol("▶ ");
    f.render_stateful_widget(list, body_area, &mut app.source_ctx.source_pr_state);
    render_list_scrollbar(
        f,
        area,
        total * 2,
        body_area.height,
        app.source_ctx.source_pr_state.offset(),
    );
}

pub(crate) fn draw_source_issues(f: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focus == Column::Repos;
    let border_style = panel_focus(focused);

    let loading_suffix = match &app.loading {
        Some(LoadingKind::Issues) => " ⟳".to_string(),
        Some(LoadingKind::Action(a)) => format!(" {a}…"),
        _ => String::new(),
    };
    let source_name = app
        .selected_source()
        .map(|s| s.display())
        .unwrap_or_default();
    let issue_count_suffix = if app.filter_active || !app.source_ctx.source_issue_filter.is_empty()
    {
        format!(
            "  {}/{}",
            app.visible_source_issues().len(),
            app.source_ctx.source_issues.len()
        )
    } else {
        String::new()
    };
    let base = format!(" {source_name}{loading_suffix}{issue_count_suffix} ");
    let title = filter_title(
        &base,
        &app.source_ctx.source_issue_filter,
        app.filter_active && app.focus == Column::Repos,
        focused,
    );
    let block = panel_block(title, border_style).title_bottom(repos_tab_line(
        ReposView::IssueList,
        app.source_ctx.source_prs.len(),
        app.source_ctx.source_prs_pagination.has_more,
        app.source_ctx.source_issues.len(),
        app.source_ctx.source_issues_pagination.has_more,
    ));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let [header_area, body_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(inner);

    let inner_width = area.width.saturating_sub(4) as usize;
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

    let visible_issues = app.visible_source_issues();

    if visible_issues.is_empty() {
        if app.loading.is_some() {
            f.render_widget(loading_placeholder(), body_area);
        } else if !app.source_ctx.source_issue_filter.is_empty() {
            f.render_widget(dim_italic("no results"), body_area);
        } else {
            f.render_widget(dim_italic("(no open issues)"), body_area);
        }
        return;
    }

    let age_col = 4usize;
    let author_col = visible_issues
        .iter()
        .map(|i| i.author.len())
        .max()
        .unwrap_or(6)
        .clamp(6, 20);

    let selected_idx = app.source_ctx.source_issue_state.selected();
    let items: Vec<ListItem> = visible_issues
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

            let repo_num = format!("{} #{} ", issue.repo, issue.number);
            let repo_num_w = repo_num.len();
            let age = relative_time(&issue.created_at);
            let author_str = format!("@{:<acol$}", issue.author, acol = author_col);
            let age_str = format!("  {ICON_CLOCK} {age:>age_col$}");
            let author_age_w = author_str.width() + 2 + 1 + 1 + age_col;
            let title_budget = inner_width.saturating_sub(repo_num_w + author_age_w + 1);
            let title_text = truncate(&issue.title, title_budget);
            let title_w = title_text.width();
            let gap = inner_width.saturating_sub(repo_num_w + title_w + author_age_w);

            let line1 = Line::from(vec![
                Span::styled(repo_num, item_style(focused).add_modifier(Modifier::BOLD)),
                Span::styled(title_text, item_style(focused)),
                gap_span(gap),
                Span::styled(
                    author_str,
                    Style::new().fg(meta_fg).add_modifier(Modifier::BOLD),
                ),
                Span::styled(age_str, Style::new().fg(meta_fg)),
            ])
            .style(hl);

            let (state_icon, state_color) = if issue.state == "closed" {
                (ICON_PR_CLOSED, Color::Red)
            } else {
                (ICON_ISSUE_OPEN, Color::Green)
            };
            let icon_line = Line::from(vec![
                Span::raw("  "),
                Span::styled(state_icon, Style::new().fg(state_color)),
            ])
            .style(hl);

            let mut text_lines = vec![line1, icon_line];
            text_lines.extend(
                wrap_label_lines(&issue.labels, inner_width, cap_bg)
                    .into_iter()
                    .map(|line| line.style(hl)),
            );
            ListItem::new(Text::from(text_lines))
        })
        .collect();

    let total = items.len();
    let list = List::new(items).highlight_symbol("▶ ");
    f.render_stateful_widget(list, body_area, &mut app.source_ctx.source_issue_state);
    render_list_scrollbar(
        f,
        area,
        total * 2,
        body_area.height,
        app.source_ctx.source_issue_state.offset(),
    );
}

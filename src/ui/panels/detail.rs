use super::{
    StatusLike, active_style, diff_stat_spans, dim_italic, draw_scrollable_body, inactive_style,
    label_pill_spans, label_pill_w, list_highlight_style, loading_placeholder,
    mergeable_state_span, pad_to_width,
};
use crate::{
    app::App,
    types::{CheckStatus, Column, DetailSection, RepoId},
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

pub(crate) fn draw_pr_detail(f: &mut Frame, app: &mut App, area: Rect) {
    let in_detail = app.focus == Column::Detail;
    let pr = app.selected_pr();
    let detail_rid = app.selected_owner_repo();
    let detail_owner = detail_rid.as_ref().map_or("", |r| &r.owner);
    let detail_repo = detail_rid.as_ref().map_or("", |r| &r.repo);
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

    let Some(pr) = pr else {
        f.render_widget(
            Paragraph::new("Select a PR").style(Style::new().fg(Color::DarkGray)),
            inner,
        );
        return;
    };
    let title_lines = u16::try_from(
        Paragraph::new(pr.title.as_str())
            .wrap(Wrap { trim: false })
            .line_count(inner.width),
    )
    .unwrap_or(1)
    .max(1);

    // Build non-label meta spans first, then pack label pills onto lines manually.
    // Manual packing prevents ratatui's paragraph wrapper from splitting a pill's
    // three spans (left-cap, text, right-cap) across two display lines.
    let mut meta_prefix: Vec<Span> = vec![];
    meta_prefix.push(Span::styled(
        format!("@{}", pr.author),
        Style::new().fg(Color::Cyan),
    ));
    if let Some((add_span, del_span)) = diff_stat_spans(pr) {
        if !meta_prefix.is_empty() {
            meta_prefix.push(Span::raw("  "));
        }
        meta_prefix.extend([add_span, Span::raw(" "), del_span]);
    }
    if let Some(s) = mergeable_state_span(
        app.repo_ctx
            .mergeable_states
            .get(&RepoId::new(detail_owner, detail_repo).pr(pr.number)),
    ) {
        if !meta_prefix.is_empty() {
            meta_prefix.push(Span::raw("  "));
        }
        meta_prefix.push(s);
    }
    if !pr.head_ref.is_empty() {
        if !meta_prefix.is_empty() {
            meta_prefix.push(Span::raw("  "));
        }
        meta_prefix.push(Span::styled(
            format!("{} \u{2192} {}", pr.head_ref, pr.base_ref),
            Style::new().fg(Color::DarkGray),
        ));
    }
    if !pr.requested_reviewers.is_empty() {
        if !meta_prefix.is_empty() {
            meta_prefix.push(Span::raw("  "));
        }
        meta_prefix.push(Span::styled(
            format!("👁 {}", pr.requested_reviewers.join(", ")),
            Style::new().fg(Color::Magenta),
        ));
    }
    let width = inner.width as usize;
    let mut meta_lines: Vec<Line> = vec![];
    let mut cur_spans: Vec<Span> = meta_prefix;
    let mut cur_w: usize = cur_spans.iter().map(Span::width).sum();
    let mut label_started = false;
    for lbl in &pr.labels {
        let pill_w = label_pill_w(lbl);
        let sep_w = if cur_w == 0 {
            0
        } else if !label_started {
            2
        } else {
            1
        };
        if cur_w > 0 && cur_w + sep_w + pill_w > width {
            meta_lines.push(pad_to_width(std::mem::take(&mut cur_spans), cur_w, width));
            cur_w = 0;
        } else if sep_w > 0 {
            cur_spans.push(Span::raw(" ".repeat(sep_w)));
            cur_w += sep_w;
        }
        cur_spans.extend(label_pill_spans(lbl));
        cur_w += pill_w;
        label_started = true;
    }
    if !cur_spans.is_empty() {
        meta_lines.push(pad_to_width(cur_spans, cur_w, width));
    }
    let meta_line_count = u16::try_from(meta_lines.len()).unwrap_or(0);
    let header_height = title_lines + meta_line_count;

    let body_focusable = app.pr_body_focusable();
    let checks_focusable = app.checks_focusable();
    let body_constraint = if body_focusable {
        Constraint::Min(3)
    } else {
        Constraint::Length(3)
    };
    let bar_runs = app.repo_ctx.check_runs.as_deref().unwrap_or(&[]);
    let has_bar = !bar_runs.is_empty();

    let checks_constraint = if checks_focusable {
        if body_focusable {
            let max_h = (inner.height * 2 / 5).max(4);
            let h = match app.repo_ctx.check_runs.as_deref() {
                Some(runs) if !runs.is_empty() => {
                    // 2 borders + 1 bar + list items
                    (2 + 1 + runs.len() as u16).min(max_h).max(4)
                }
                _ => max_h,
            };
            Constraint::Length(h)
        } else {
            Constraint::Min(4)
        }
    } else {
        Constraint::Length(3)
    };

    let [header_area, body_area, checks_area] = Layout::vertical([
        Constraint::Length(header_height),
        body_constraint,
        checks_constraint,
    ])
    .areas(inner);

    // Title + meta header
    let title_line = Line::from(Span::styled(
        pr.title.clone(),
        Style::new().fg(Color::White).add_modifier(Modifier::BOLD),
    ));
    let mut header_lines = vec![title_line];
    header_lines.extend(meta_lines);
    f.render_widget(Clear, inner);
    f.render_widget(
        Paragraph::new(Text::from(header_lines)).wrap(Wrap { trim: false }),
        header_area,
    );

    let body_active = in_detail && app.repo_ctx.detail_section == DetailSection::Body;
    let checks_active = in_detail && app.repo_ctx.detail_section == DetailSection::Checks;

    // Body section
    let body_style = if body_active {
        active_style()
    } else {
        inactive_style()
    };
    let body_block = Block::default()
        .title(if body_active && checks_focusable {
            " Description  Tab\u{2192} "
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
        app.repo_ctx.pr_body.as_ref(),
        app.repo_ctx.pr_body_scroll,
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
            " Checks  Tab\u{2192} "
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

    match &app.repo_ctx.check_runs {
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
                    let (icon, color) = (run.status.icon(), run.status.color());
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
            f.render_stateful_widget(list, list_area, &mut app.repo_ctx.check_runs_state);
            if runs.len() > list_area.height as usize {
                let mut sb = ScrollbarState::new(runs.len())
                    .position(app.repo_ctx.check_runs_state.offset());
                f.render_stateful_widget(
                    Scrollbar::new(ScrollbarOrientation::VerticalRight),
                    checks_area,
                    &mut sb,
                );
            }
        }
    }
}

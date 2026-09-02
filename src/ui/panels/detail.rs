use super::ICON_PR_DRAFT;
use super::{
    ICON_CHECKLIST, ICON_COMMENT, ICON_COMMIT, ICON_DOT, ICON_FILE_ADDED, ICON_FILE_DIFF,
    ICON_FILE_MODIFIED, ICON_FILE_REMOVED, ICON_FILE_RENAMED, ICON_PR_HEADER, StatusLike,
    active_style, detail_tab_line, diff_stat_spans, dim_italic, draw_scrollable_body,
    inactive_style, label_pill_spans, label_pill_w, list_highlight_style, loading_placeholder,
    mergeable_state_span, pad_to_width, relative_time, truncate,
};
use crate::{
    app::App,
    types::{CheckCategory, CheckStatus, Column, DetailSection, RepoId},
};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{
        Block, Borders, Clear, List, ListItem, ListState, Paragraph, Scrollbar,
        ScrollbarOrientation, ScrollbarState, Wrap,
    },
};
use unicode_width::UnicodeWidthStr;

/// Proportional bar spans filling `width` columns, colored by check status counts.
fn checks_bar_spans(runs: &[crate::types::CheckRun], width: usize) -> Vec<Span<'static>> {
    if runs.is_empty() || width == 0 {
        return vec![];
    }
    let mut counts = [0usize; 5]; // [failing, cancelled, pending, unknown, passing]
    for r in runs {
        match r.status {
            CheckStatus::Failing => counts[0] += 1,
            CheckStatus::Cancelled => counts[1] += 1,
            CheckStatus::Pending => counts[2] += 1,
            CheckStatus::Unknown => counts[3] += 1,
            CheckStatus::Passing => counts[4] += 1,
        }
    }
    let total = runs.len();
    let colors = [
        Color::Red,
        Color::Gray,
        Color::Yellow,
        Color::DarkGray,
        Color::Green,
    ];
    let last = counts.len() - 1;
    let mut spans = Vec::new();
    let mut used = 0usize;
    for (i, (&count, &color)) in counts.iter().zip(colors.iter()).enumerate() {
        if count == 0 {
            continue;
        }
        let cols = if i == last {
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
    let pr_repo = pr.and_then(|pr| {
        (!pr.repo.is_empty())
            .then_some(pr.repo.as_str())
            .or((!detail_repo.is_empty()).then_some(detail_repo))
    });
    let title = match (pr.map(|pr| pr.number), pr_repo) {
        (Some(n), Some(repo)) => format!(" {repo} #{n} "),
        (Some(n), None) => format!(" PR #{n} "),
        _ => " Detail ".to_string(),
    };
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

    let width = inner.width as usize;

    // Build non-label meta spans first, then pack label pills onto lines manually.
    // Manual packing prevents ratatui's paragraph wrapper from splitting a pill's
    // three spans (left-cap, text, right-cap) across two display lines.
    let mut meta_prefix: Vec<Span> = vec![];
    if pr.draft {
        meta_prefix.push(Span::styled(
            format!("{} Draft", ICON_PR_DRAFT.trim_end()),
            Style::new().fg(Color::DarkGray),
        ));
    }
    if !meta_prefix.is_empty() {
        meta_prefix.push(Span::raw("  "));
    }
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
        let prefix_w: usize = meta_prefix.iter().map(Span::width).sum();
        let branch_text = format!("{} \u{2192} {}", pr.head_ref, pr.base_ref);
        let branch_text = truncate(&branch_text, width.saturating_sub(prefix_w));
        meta_prefix.push(Span::styled(branch_text, Style::new().fg(Color::DarkGray)));
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
        cur_spans.extend(label_pill_spans(lbl, Color::Reset));
        cur_w += pill_w;
        label_started = true;
    }
    if !cur_spans.is_empty() {
        meta_lines.push(pad_to_width(cur_spans, cur_w, width));
    }
    let meta_line_count = u16::try_from(meta_lines.len()).unwrap_or(0);
    let header_height = title_lines + meta_line_count;

    let [header_area, rest_area] =
        Layout::vertical([Constraint::Length(header_height), Constraint::Min(0)]).areas(inner);

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

    let [tabs_area, content_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(rest_area);

    let checks_count = app.repo_ctx.check_runs.as_deref().map_or(0, <[_]>::len);
    let activity_count = app.repo_ctx.pr_activity.as_deref().map_or(0, <[_]>::len);
    let commits_count = app.repo_ctx.pr_commits.as_deref().map_or(0, <[_]>::len);
    f.render_widget(
        Paragraph::new(detail_tab_line(
            app.repo_ctx.detail_section,
            checks_count,
            activity_count,
            commits_count,
        )),
        tabs_area,
    );

    let section_style = if in_detail {
        active_style()
    } else {
        inactive_style()
    };

    match app.repo_ctx.detail_section {
        DetailSection::Overview => {
            let block = Block::default()
                .title(format!(" {ICON_PR_HEADER} Overview "))
                .title_style(section_style)
                .borders(Borders::ALL)
                .border_style(section_style);
            let overview_inner = block.inner(content_area);
            f.render_widget(block, content_area);

            let heading = |text: &'static str| {
                Line::from(Span::styled(
                    text,
                    Style::new()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                ))
            };
            let dim = || Style::new().fg(Color::DarkGray);
            let dim_italic_style = || {
                Style::new()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::ITALIC)
            };

            let mut lines: Vec<Line> = vec![heading("Summary"), Line::raw("")];
            match &app.repo_ctx.pr_body {
                None => lines.push(Line::styled("Loading…", dim())),
                Some(b) if b.is_empty() => {
                    lines.push(Line::styled("(no description)", dim_italic_style()));
                }
                Some(b) => lines.extend(super::render_markdown(b).lines),
            }

            lines.push(Line::raw(""));
            lines.push(heading("Changes"));
            lines.push(Line::raw(""));
            match (&app.repo_ctx.pr_files, &app.repo_ctx.pr_commits) {
                (None, _) | (_, None) => lines.push(Line::styled("Loading…", dim())),
                (Some(files), Some(commits)) => {
                    let mut spans = vec![Span::raw(format!(
                        "{} file{} changed  ",
                        files.len(),
                        if files.len() == 1 { "" } else { "s" }
                    ))];
                    if let Some((add, del)) = diff_stat_spans(pr) {
                        spans.push(add);
                        spans.push(Span::raw(" "));
                        spans.push(del);
                    }
                    lines.push(Line::from(spans));
                    lines.push(Line::from(vec![
                        Span::raw(format!(
                            "{} commit{}",
                            commits.len(),
                            if commits.len() == 1 { "" } else { "s" }
                        )),
                        Span::styled(
                            format!("  updated {}", relative_time(&pr.updated_at, app.now())),
                            dim(),
                        ),
                    ]));
                }
            }

            lines.push(Line::raw(""));
            lines.push(heading("Checks"));
            lines.push(Line::raw(""));
            match &app.repo_ctx.check_runs {
                None => lines.push(Line::styled("Loading…", dim())),
                Some(runs) if runs.is_empty() => {
                    lines.push(Line::styled("(no checks)", dim_italic_style()));
                }
                Some(runs) => {
                    let (mut failing, mut pending, mut passing) = (0usize, 0usize, 0usize);
                    for run in runs {
                        match run.status.category() {
                            CheckCategory::Failing => failing += 1,
                            CheckCategory::Pending => pending += 1,
                            CheckCategory::Passing => passing += 1,
                        }
                    }
                    let (headline, color) = if failing > 0 {
                        ("Some checks were not successful", Color::Red)
                    } else if pending > 0 {
                        ("Some checks haven't completed yet", Color::Yellow)
                    } else {
                        ("All checks have passed", Color::Green)
                    };
                    lines.push(Line::styled(headline, Style::new().fg(color)));
                    let mut parts = Vec::with_capacity(3);
                    if failing > 0 {
                        parts.push(format!("{failing} failing"));
                    }
                    if pending > 0 {
                        parts.push(format!("{pending} in progress"));
                    }
                    if passing > 0 {
                        parts.push(format!("{passing} successful"));
                    }
                    lines.push(Line::styled(parts.join(", "), dim()));
                    lines.push(Line::from(checks_bar_spans(
                        runs,
                        overview_inner.width as usize,
                    )));
                }
            }

            let text = Text::from(lines);
            let total_lines = Paragraph::new(text.clone())
                .wrap(Wrap { trim: false })
                .line_count(overview_inner.width);
            f.render_widget(
                Paragraph::new(text)
                    .wrap(Wrap { trim: false })
                    .scroll((app.repo_ctx.pr_body_scroll, 0)),
                overview_inner,
            );
            if total_lines > overview_inner.height as usize {
                let mut sb =
                    ScrollbarState::new(total_lines).position(app.repo_ctx.pr_body_scroll as usize);
                f.render_stateful_widget(
                    Scrollbar::new(ScrollbarOrientation::VerticalRight),
                    content_area,
                    &mut sb,
                );
            }
        }

        DetailSection::Activity => {
            let block = Block::default()
                .title(format!(" {ICON_COMMENT} Activity "))
                .title_style(section_style)
                .borders(Borders::ALL)
                .border_style(section_style);
            let activity_inner = block.inner(content_area);
            f.render_widget(block, content_area);

            let now = app.now();
            let formatted: Option<String> = app.repo_ctx.pr_activity.as_ref().map(|comments| {
                let mut s = String::new();
                for comment in comments {
                    s.push_str(&format!(
                        "### @{} \u{b7} {}\n\n{}\n\n---\n\n",
                        comment.author,
                        relative_time(&comment.created_at, now),
                        comment.body
                    ));
                }
                s
            });

            draw_scrollable_body(
                f,
                formatted.as_ref(),
                "(no comments)",
                app.repo_ctx.pr_activity_scroll,
                activity_inner,
                content_area,
            );
        }

        DetailSection::Commits => {
            let block = Block::default()
                .title(format!(" {ICON_COMMIT} Commits "))
                .title_style(section_style)
                .borders(Borders::ALL)
                .border_style(section_style);
            let commits_inner = block.inner(content_area);
            f.render_widget(block, content_area);

            match &app.repo_ctx.pr_commits {
                None => f.render_widget(loading_placeholder(), commits_inner),
                Some(commits) if commits.is_empty() => {
                    f.render_widget(dim_italic("(no commits)"), commits_inner);
                }
                Some(commits) => {
                    let width = commits_inner.width as usize;
                    let now = app.now();
                    let items: Vec<ListItem> = commits
                        .iter()
                        .map(|commit| {
                            let short_sha = commit.sha.get(..7).unwrap_or(commit.sha.as_str());
                            let first_line = commit.message.lines().next().unwrap_or("");
                            let trailing = format!(
                                "{} \u{b7} {}",
                                commit.author,
                                relative_time(&commit.date, now)
                            );
                            let prefix_w = ICON_COMMIT.width() + 1 + short_sha.width() + 1;
                            let trailing_w = trailing.width() + 2;
                            let mut spans = vec![Span::styled(
                                format!("{ICON_COMMIT} {short_sha} "),
                                Style::new().fg(Color::Cyan),
                            )];
                            if prefix_w + first_line.width().min(1) + trailing_w <= width {
                                let msg_budget =
                                    width.saturating_sub(prefix_w).saturating_sub(trailing_w);
                                let msg = truncate(first_line, msg_budget);
                                let used = prefix_w + msg.width();
                                let pad = width.saturating_sub(used + trailing_w);
                                spans.push(Span::styled(msg, Style::new().fg(Color::White)));
                                if pad > 0 {
                                    spans.push(Span::raw(" ".repeat(pad)));
                                }
                                spans.push(Span::styled(
                                    format!("  {trailing}"),
                                    Style::new().fg(Color::DarkGray),
                                ));
                            } else {
                                let msg_budget = width.saturating_sub(prefix_w);
                                spans.push(Span::styled(
                                    truncate(first_line, msg_budget),
                                    Style::new().fg(Color::White),
                                ));
                            }
                            Line::from(spans).into()
                        })
                        .collect();
                    let list = List::new(items)
                        .highlight_style(list_highlight_style(in_detail))
                        .highlight_symbol("▶ ");
                    f.render_stateful_widget(
                        list,
                        commits_inner,
                        &mut app.repo_ctx.pr_commits_state,
                    );
                    if commits.len() > commits_inner.height as usize {
                        let mut sb = ScrollbarState::new(commits.len())
                            .position(app.repo_ctx.pr_commits_state.offset());
                        f.render_stateful_widget(
                            Scrollbar::new(ScrollbarOrientation::VerticalRight),
                            content_area,
                            &mut sb,
                        );
                    }
                }
            }
        }

        DetailSection::Checks => {
            let checks_block = Block::default()
                .title(format!(" {ICON_CHECKLIST} Checks "))
                .title_style(section_style)
                .borders(Borders::ALL)
                .border_style(section_style);
            let checks_inner = checks_block.inner(content_area);
            f.render_widget(checks_block, content_area);

            let bar_runs = app.repo_ctx.check_runs.as_deref().unwrap_or(&[]);
            let has_bar = !bar_runs.is_empty();

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
                    // `runs` is already grouped by category (handlers.rs sorts on arrival), so a
                    // single pass can insert a header whenever the category changes and, in the
                    // same pass, translate the persisted raw check-runs index into this
                    // header-inclusive display index for highlighting.
                    let selected_raw = app.repo_ctx.check_runs_state.selected();
                    let mut items: Vec<ListItem> = Vec::with_capacity(runs.len() + 3);
                    let mut last_category: Option<CheckCategory> = None;
                    let mut display_selected: Option<usize> = None;
                    for (i, run) in runs.iter().enumerate() {
                        let category = run.status.category();
                        if last_category != Some(category) {
                            items.push(ListItem::new(Line::from(Span::styled(
                                category.label(),
                                Style::new()
                                    .fg(Color::DarkGray)
                                    .add_modifier(Modifier::BOLD),
                            ))));
                            last_category = Some(category);
                        }
                        if selected_raw == Some(i) {
                            display_selected = Some(items.len());
                        }
                        let (icon, color) = (run.status.icon(), run.status.color());
                        items.push(
                            Line::from(vec![
                                Span::styled(format!("{icon} "), Style::new().fg(color)),
                                Span::styled(run.name.clone(), Style::new().fg(Color::White)),
                            ])
                            .into(),
                        );
                    }
                    let total = items.len();
                    let list = List::new(items)
                        .highlight_style(list_highlight_style(in_detail))
                        .highlight_symbol("▶ ");
                    let mut render_state = ListState::default();
                    render_state.select(display_selected);
                    f.render_stateful_widget(list, list_area, &mut render_state);
                    if total > list_area.height as usize {
                        let mut sb = ScrollbarState::new(total).position(render_state.offset());
                        f.render_stateful_widget(
                            Scrollbar::new(ScrollbarOrientation::VerticalRight),
                            content_area,
                            &mut sb,
                        );
                    }
                }
            }
        }

        DetailSection::FilesChanged => {
            let block = Block::default()
                .title(format!(" {ICON_FILE_DIFF} Files Changed "))
                .title_style(section_style)
                .borders(Borders::ALL)
                .border_style(section_style);
            let files_inner = block.inner(content_area);
            f.render_widget(block, content_area);

            match &app.repo_ctx.pr_files {
                None => f.render_widget(loading_placeholder(), files_inner),
                Some(files) if files.is_empty() => {
                    f.render_widget(dim_italic("(no file changes)"), files_inner);
                }
                Some(files) => {
                    let width = files_inner.width as usize;
                    let items: Vec<ListItem> = files
                        .iter()
                        .map(|file| {
                            let (icon, color) = match file.status.as_str() {
                                "added" => (ICON_FILE_ADDED, Color::Green),
                                "modified" | "changed" => (ICON_FILE_MODIFIED, Color::Yellow),
                                "removed" | "deleted" => (ICON_FILE_REMOVED, Color::Red),
                                "renamed" => (ICON_FILE_RENAMED, Color::Blue),
                                _ => (ICON_DOT, Color::Gray),
                            };
                            let add_span = Span::styled(
                                format!("+{}", file.additions),
                                Style::new().fg(Color::Green),
                            );
                            let del_span = Span::styled(
                                format!(" -{}", file.deletions),
                                Style::new().fg(Color::Red),
                            );
                            let trailing_w = add_span.width() + del_span.width();
                            let prefix_w = icon.width() + 1;
                            let name_budget = width
                                .saturating_sub(prefix_w)
                                .saturating_sub(trailing_w.saturating_add(1));
                            let name = truncate(&file.filename, name_budget);
                            let used = prefix_w + name.width();
                            let pad = width.saturating_sub(used + trailing_w);
                            let mut spans = vec![
                                Span::styled(format!("{icon} "), Style::new().fg(color)),
                                Span::styled(name, Style::new().fg(Color::White)),
                            ];
                            if pad > 0 {
                                spans.push(Span::raw(" ".repeat(pad)));
                            }
                            spans.push(add_span);
                            spans.push(del_span);
                            Line::from(spans).into()
                        })
                        .collect();
                    let list = List::new(items)
                        .highlight_style(list_highlight_style(in_detail))
                        .highlight_symbol("▶ ");
                    f.render_stateful_widget(list, files_inner, &mut app.repo_ctx.pr_files_state);
                    if files.len() > files_inner.height as usize {
                        let mut sb = ScrollbarState::new(files.len())
                            .position(app.repo_ctx.pr_files_state.offset());
                        f.render_stateful_widget(
                            Scrollbar::new(ScrollbarOrientation::VerticalRight),
                            content_area,
                            &mut sb,
                        );
                    }
                }
            }
        }
    }
}

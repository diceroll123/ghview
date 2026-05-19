use super::render_list_scrollbar;
use crate::{
    app::{App, DEPENDABOT_COMMANDS},
    keys::{
        Action, CHECKS_BINDINGS, NAV_ACTIONS, PRS_BAR, PRS_BINDINGS, REPOS_BAR, SOURCES_BAR,
        find_binding,
    },
};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, Wrap},
};

pub(super) fn draw_help(f: &mut Frame, app: &App, area: Rect) {
    let bar_entries = |actions: &[Action]| -> Vec<(String, String)> {
        actions
            .iter()
            .filter(|&&a| app.action_permitted(a))
            .filter_map(|&a| find_binding(a).map(|b| (b.display.to_string(), b.label.to_string())))
            .collect()
    };

    let section_style = Style::new().fg(Color::White).add_modifier(Modifier::BOLD);
    let key_style = Style::new().fg(Color::Yellow);
    let label_style = Style::new().fg(Color::White);
    let dim_style = Style::new().fg(Color::DarkGray);

    let make_panel = |sections: Vec<(&str, Vec<(String, String)>)>| -> Vec<Row<'static>> {
        let cap = sections.iter().map(|(_, e)| e.len() + 2).sum::<usize>();
        let mut rows: Vec<Row> = Vec::with_capacity(cap);
        for (title, entries) in sections {
            if !rows.is_empty() {
                rows.push(Row::new([Cell::from(""), Cell::from("")]));
            }
            rows.push(Row::new([
                Cell::from(""),
                Cell::from(Span::styled(title.to_string(), section_style)),
            ]));
            for (key, label) in entries {
                rows.push(Row::new([
                    Cell::from(Span::styled(key, key_style)),
                    Cell::from(Span::styled(label, label_style)),
                ]));
            }
        }
        rows
    };

    let mut pr_entries = bar_entries(PRS_BAR);
    for b in PRS_BINDINGS {
        if !pr_entries.iter().any(|(d, _)| d == b.display) && app.action_permitted(b.action) {
            pr_entries.push((b.display.to_string(), b.label.to_string()));
        }
    }

    let checks_entries: Vec<(String, String)> = CHECKS_BINDINGS
        .iter()
        .map(|b| (b.display.to_string(), b.label.to_string()))
        .collect();

    let kb = &app.config.keybindings;
    let custom_kv = |list: &[crate::config::Keybinding]| -> Vec<(String, String)> {
        list.iter()
            .map(|k| {
                (
                    k.key.clone(),
                    k.name
                        .as_deref()
                        .or(k.builtin.as_deref())
                        .unwrap_or("custom")
                        .to_string(),
                )
            })
            .collect()
    };

    let left_sections: Vec<(&str, Vec<(String, String)>)> = vec![
        ("\u{f14e}  Navigation", bar_entries(NAV_ACTIONS)),
        ("\u{f0c0}  Sources", bar_entries(SOURCES_BAR)),
        ("\u{e702}  Browse", bar_entries(REPOS_BAR)),
    ];
    let mut right_sections: Vec<(&str, Vec<(String, String)>)> = vec![
        ("\u{f407}  PRs", pr_entries),
        ("\u{e641}  Checks", checks_entries),
    ];
    if !kb.universal.is_empty() {
        right_sections.push(("\u{f013}  Universal (custom)", custom_kv(&kb.universal)));
    }
    if !kb.repos.is_empty() {
        right_sections.push(("\u{f013}  Browse (custom)", custom_kv(&kb.repos)));
    }
    if !kb.prs.is_empty() {
        right_sections.push(("\u{f013}  PRs (custom)", custom_kv(&kb.prs)));
    }
    if !kb.checks.is_empty() {
        right_sections.push(("\u{f013}  Checks (custom)", custom_kv(&kb.checks)));
    }

    let left_rows = make_panel(left_sections);
    let right_rows = make_panel(right_sections);

    let total_rows = left_rows.len().max(right_rows.len());
    let popup_width = 80u16;
    let popup_height = (u16::try_from(total_rows)
        .unwrap_or(u16::MAX)
        .saturating_add(4))
    .max(20)
    .min(area.height.saturating_sub(2));
    let visible_rows = popup_height.saturating_sub(4) as usize;
    let max_scroll = u16::try_from(total_rows.saturating_sub(visible_rows)).unwrap_or(u16::MAX);
    let scroll = app.help_scroll.min(max_scroll) as usize;

    let x = area.width.saturating_sub(popup_width) / 2;
    let y = area.height.saturating_sub(popup_height) / 2;
    let popup_area = Rect::new(x, y, popup_width.min(area.width), popup_height);

    f.render_widget(Clear, popup_area);
    let scrollable = total_rows > visible_rows;
    let hint_text = if scrollable {
        format!(
            "j/k scroll  ({}/{})",
            scroll + visible_rows.min(total_rows),
            total_rows
        )
    } else {
        "press any key to close".to_string()
    };
    let block = Block::default()
        .title(" Help ")
        .borders(Borders::ALL)
        .border_style(Style::new().fg(Color::Yellow));

    let inner = block.inner(popup_area);
    f.render_widget(block, popup_area);

    let [content_area, hint_line] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(inner);
    let [left_area, right_area] =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
            .areas(content_area);

    let col_widths = [Constraint::Length(5), Constraint::Min(0)];
    f.render_widget(
        Table::new(left_rows.into_iter().skip(scroll), col_widths),
        left_area,
    );
    f.render_widget(
        Table::new(right_rows.into_iter().skip(scroll), col_widths),
        right_area,
    );
    f.render_widget(
        Paragraph::new(Span::styled(hint_text, dim_style)),
        hint_line,
    );
    render_list_scrollbar(f, popup_area, total_rows, visible_rows as u16, scroll);
}

pub(super) fn draw_dependabot_menu(f: &mut Frame, area: Rect) {
    use std::fmt::Write;
    let mut text = String::from("🤖 Dependabot Commands\n\n");
    for (key, cmd) in DEPENDABOT_COMMANDS {
        let _ = writeln!(text, "  {key}   @dependabot {cmd}");
    }
    text.push_str("\nPress any other key to cancel");
    let popup_width = 50u16;
    let popup_height = u16::try_from(DEPENDABOT_COMMANDS.len() + 6).unwrap_or(u16::MAX);
    let x = area.width.saturating_sub(popup_width) / 2;
    let y = area.height.saturating_sub(popup_height) / 2;
    let popup_area = Rect::new(
        x,
        y,
        popup_width.min(area.width),
        popup_height.min(area.height),
    );
    f.render_widget(Clear, popup_area);
    let block = Block::default()
        .title(" Dependabot ")
        .borders(Borders::ALL)
        .border_style(Style::new().fg(Color::Green));
    let para = Paragraph::new(text.as_str())
        .block(block)
        .wrap(Wrap { trim: false })
        .style(Style::new().fg(Color::White));
    f.render_widget(para, popup_area);
}

pub(super) fn draw_diff(f: &mut Frame, app: &App, area: Rect) {
    use ratatui::text::Line;
    let Some(diff) = &app.repo_ctx.diff_view else {
        return;
    };
    let block = Block::default()
        .title(format!(" diff: {} ", diff.title))
        .borders(Borders::ALL)
        .border_style(Style::new().fg(Color::Cyan));

    let lines: Vec<Line> = diff
        .lines
        .iter()
        .map(|raw| {
            if raw.starts_with("+++")
                || raw.starts_with("---")
                || raw.starts_with("diff ")
                || raw.starts_with("index ")
                || raw.starts_with("new file")
                || raw.starts_with("deleted file")
            {
                Line::from(raw.as_str()).style(Style::new().fg(Color::Yellow).bold())
            } else if raw.starts_with("@@") {
                Line::from(raw.as_str()).style(Style::new().fg(Color::Cyan))
            } else if raw.starts_with('+') {
                Line::from(raw.as_str()).style(Style::new().fg(Color::Green))
            } else if raw.starts_with('-') {
                Line::from(raw.as_str()).style(Style::new().fg(Color::Red))
            } else {
                Line::from(raw.as_str()).style(Style::new().fg(Color::Gray))
            }
        })
        .collect();

    let total_lines = diff.lines.len();
    let para = ratatui::widgets::Paragraph::new(lines)
        .block(block)
        .scroll((diff.scroll, 0));
    f.render_widget(Clear, area);
    f.render_widget(para, area);
    render_list_scrollbar(
        f,
        area,
        total_lines,
        area.height.saturating_sub(2),
        diff.scroll as usize,
    );
}

use super::{
    ICON_ORG, ICON_ORG_GLYPH, ICON_USER, ICON_USER_GLYPH, draw_strip_vertical, filter_title,
    inactive_style, list_highlight_style, panel_block, panel_focus, render_list_scrollbar,
};
use crate::{
    app::App,
    types::{Column, LoadingKind, Source},
};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

pub(crate) fn draw_sources(f: &mut Frame, app: &mut App, area: Rect) {
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
        .highlight_style(list_highlight_style(focused))
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

pub(crate) fn draw_sources_strip(f: &mut Frame, app: &App, area: Rect) {
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

use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
};

pub fn render(src: &str) -> Text<'static> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut spans: Vec<Span<'static>> = Vec::new();

    // Style stack — each entry adds to the accumulated modifier set
    let mut bold = 0u8;
    let mut italic = 0u8;
    let mut strike = 0u8;
    let mut in_code_block = false;
    let mut list_depth = 0u8;
    let mut ordered_counters: Vec<u64> = Vec::new();
    let mut in_heading = false;
    let mut heading_color = Color::White;

    let opts = Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS;
    let parser = Parser::new_ext(src, opts);

    let flush = |spans: &mut Vec<Span<'static>>, lines: &mut Vec<Line<'static>>| {
        lines.push(Line::from(std::mem::take(spans)));
    };

    let current_style =
        |bold: u8, italic: u8, strike: u8, in_heading: bool, heading_color: Color| -> Style {
            let mut s = Style::default();
            if in_heading || bold > 0 {
                s = s.add_modifier(Modifier::BOLD);
            }
            if italic > 0 {
                s = s.add_modifier(Modifier::ITALIC);
            }
            if strike > 0 {
                s = s.add_modifier(Modifier::CROSSED_OUT);
            }
            if in_heading {
                s = s.fg(heading_color);
            }
            s
        };

    for event in parser {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                if !spans.is_empty() {
                    flush(&mut spans, &mut lines);
                }
                in_heading = true;
                heading_color = match level {
                    HeadingLevel::H1 => Color::Cyan,
                    HeadingLevel::H2 => Color::LightBlue,
                    _ => Color::Blue,
                };
                let prefix = match level {
                    HeadingLevel::H1 => "# ",
                    HeadingLevel::H2 => "## ",
                    HeadingLevel::H3 => "### ",
                    _ => "#### ",
                };
                spans.push(Span::styled(
                    prefix,
                    Style::default()
                        .fg(heading_color)
                        .add_modifier(Modifier::BOLD),
                ));
            }
            Event::End(TagEnd::Heading(_)) => {
                flush(&mut spans, &mut lines);
                lines.push(Line::raw(""));
                in_heading = false;
            }

            Event::End(TagEnd::Paragraph | TagEnd::BlockQuote(_)) => {
                flush(&mut spans, &mut lines);
                lines.push(Line::raw(""));
            }

            Event::Start(Tag::Strong) => {
                bold += 1;
            }
            Event::End(TagEnd::Strong) => {
                bold = bold.saturating_sub(1);
            }

            Event::Start(Tag::Emphasis) => {
                italic += 1;
            }
            Event::End(TagEnd::Emphasis) => {
                italic = italic.saturating_sub(1);
            }

            Event::Start(Tag::Strikethrough) => {
                strike += 1;
            }
            Event::End(TagEnd::Strikethrough) => {
                strike = strike.saturating_sub(1);
            }

            Event::Start(Tag::Link { dest_url, .. }) => {
                spans.push(Span::styled("[", Style::default().fg(Color::DarkGray)));
                let _ = dest_url; // link text rendered by inner Text events
            }
            Event::End(TagEnd::Link) => {
                spans.push(Span::styled("]", Style::default().fg(Color::DarkGray)));
            }

            Event::Start(Tag::Image { .. }) => {
                spans.push(Span::styled("🖼 ", Style::default().fg(Color::DarkGray)));
            }
            Event::Code(code) => {
                spans.push(Span::styled(
                    format!("`{code}`"),
                    Style::default().fg(Color::Green),
                ));
            }

            Event::Start(Tag::CodeBlock(_)) => {
                flush(&mut spans, &mut lines);
                in_code_block = true;
            }
            Event::End(TagEnd::CodeBlock) => {
                in_code_block = false;
                lines.push(Line::raw(""));
            }

            Event::Start(Tag::List(start)) => {
                list_depth += 1;
                if let Some(n) = start {
                    ordered_counters.push(n);
                } else {
                    ordered_counters.push(0);
                }
            }
            Event::End(TagEnd::List(_)) => {
                list_depth = list_depth.saturating_sub(1);
                ordered_counters.pop();
            }

            Event::Start(Tag::Item) => {
                let indent = "  ".repeat(list_depth.saturating_sub(1) as usize);
                let bullet = if let Some(last) = ordered_counters.last_mut() {
                    if *last == 0 {
                        "• ".to_string()
                    } else {
                        let n = *last;
                        *last += 1;
                        format!("{n}. ")
                    }
                } else {
                    "• ".to_string()
                };
                spans.push(Span::styled(
                    format!("{indent}{bullet}"),
                    Style::default().fg(Color::DarkGray),
                ));
            }
            Event::End(TagEnd::Item) | Event::HardBreak => {
                flush(&mut spans, &mut lines);
            }

            Event::Start(Tag::BlockQuote(_)) => {
                spans.push(Span::styled("▌ ", Style::default().fg(Color::DarkGray)));
            }

            Event::Rule => {
                flush(&mut spans, &mut lines);
                lines.push(Line::styled(
                    "─".repeat(40),
                    Style::default().fg(Color::DarkGray),
                ));
                lines.push(Line::raw(""));
            }

            Event::SoftBreak => {
                spans.push(Span::raw(" "));
            }

            Event::Text(text) => {
                let style = if in_code_block {
                    Style::default().fg(Color::Green)
                } else {
                    current_style(bold, italic, strike, in_heading, heading_color)
                };
                if in_code_block {
                    for raw_line in text.lines() {
                        lines.push(Line::styled(raw_line.to_string(), style));
                    }
                } else {
                    spans.push(Span::styled(text.into_string(), style));
                }
            }

            Event::TaskListMarker(checked) => {
                let sym = if checked { "☑ " } else { "☐ " };
                spans.push(Span::styled(sym, Style::default().fg(Color::Yellow)));
            }

            Event::Start(Tag::TableHead | Tag::TableRow | Tag::TableCell | Tag::Table(_))
            | Event::End(
                TagEnd::TableHead | TagEnd::TableRow | TagEnd::TableCell | TagEnd::Table,
            ) => {
                // tables: treat cells as inline text with separators
            }

            Event::Html(_) | Event::InlineHtml(_) => {
                // strip HTML tags silently
            }

            _ => {}
        }
    }

    if !spans.is_empty() {
        lines.push(Line::from(spans));
    }

    Text::from(lines)
}

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
};

use crate::model::{App, Session};

pub const MIN_CARD_WIDTH: u16 = 32;
pub const MIN_CARD_HEIGHT: u16 = 10;
const CARD_GAP: u16 = 2;
const FOOTER_HEIGHT: u16 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GridLayout {
    pub columns: usize,
    pub rows: usize,
    pub cards: Vec<Rect>,
}

pub fn render(
    frame: &mut Frame<'_>,
    app: &App,
    min_card_width: u16,
    forced_columns: Option<usize>,
) {
    let area = frame.area();
    frame.render_widget(Clear, area);

    if area.width < 20 || area.height < 6 {
        render_centered_message(frame, area, "Terminal too small");
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(FOOTER_HEIGHT)])
        .split(area);

    if let Some(error) = &app.error {
        render_centered_message(frame, chunks[0], error);
    } else if app.sessions.is_empty() {
        render_centered_message(
            frame,
            chunks[0],
            "No tmux sessions found.\nPress q or Esc to quit.",
        );
    } else {
        render_grid(frame, app, chunks[0], min_card_width, forced_columns);
    }

    let footer = Paragraph::new("↑/↓/←/→ or hjkl to move · Enter to switch · q/Esc/Ctrl-C to quit")
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(footer, chunks[1]);
}

pub fn render_grid(
    frame: &mut Frame<'_>,
    app: &App,
    area: Rect,
    min_card_width: u16,
    forced_columns: Option<usize>,
) {
    let grid = calculate_grid(area, app.sessions.len(), min_card_width, forced_columns);

    for (index, card_area) in grid.cards.iter().enumerate() {
        if let Some(session) = app.sessions.get(index) {
            render_card(frame, session, index == app.selected_index, *card_area);
        }
    }
}

pub fn render_card(frame: &mut Frame<'_>, session: &Session, selected: bool, area: Rect) {
    let status = if session.attached {
        "attached"
    } else {
        "detached"
    };
    let title = format!(
        " {} ",
        truncate(&session.name, area.width.saturating_sub(12) as usize)
    );
    let block = Block::default()
        .title(Span::styled(
            title,
            Style::default().add_modifier(if selected {
                Modifier::BOLD
            } else {
                Modifier::empty()
            }),
        ))
        .title_bottom(Span::styled(
            format!(" {status} "),
            Style::default().fg(Color::DarkGray),
        ))
        .borders(Borders::ALL)
        .border_type(if selected {
            BorderType::Double
        } else {
            BorderType::Plain
        })
        .border_style(if selected {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        });

    let preview_height = area.height.saturating_sub(5) as usize;
    let mut lines = Vec::new();
    let window = session.current_window.as_deref().unwrap_or("unknown");
    lines.push(Line::from(vec![Span::styled(
        format!("{} · {} windows", window, session.window_count),
        Style::default().fg(Color::Cyan),
    )]));
    lines.push(Line::from(""));

    if session.preview_error.is_some() {
        lines.push(Line::from(Span::styled(
            "Preview unavailable",
            Style::default().fg(Color::Red),
        )));
    } else if session.preview.is_empty() {
        lines.push(Line::from(Span::styled(
            "No visible content",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        let start = session.preview.len().saturating_sub(preview_height);
        for line in session.preview.iter().skip(start) {
            let line = truncate_ansi(line, area.width.saturating_sub(4) as usize);
            lines.push(ansi_to_line(&line));
        }
    }

    let paragraph = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false })
        .style(if selected {
            Style::default().fg(Color::White)
        } else {
            Style::default()
        });

    frame.render_widget(paragraph, area);
}

pub fn calculate_grid(
    area: Rect,
    item_count: usize,
    min_card_width: u16,
    forced_columns: Option<usize>,
) -> GridLayout {
    if item_count == 0 || area.width == 0 || area.height == 0 {
        return GridLayout {
            columns: 1,
            rows: 0,
            cards: Vec::new(),
        };
    }

    let automatic_columns = (area.width.saturating_add(CARD_GAP)
        / min_card_width.max(1).saturating_add(CARD_GAP))
    .max(1) as usize;
    let columns = forced_columns
        .unwrap_or(automatic_columns)
        .min(item_count)
        .min(u16::MAX as usize)
        .max(1);
    let rows = item_count.div_ceil(columns);
    let total_gap_width = CARD_GAP.saturating_mul(columns.saturating_sub(1) as u16);
    let card_width = area
        .width
        .saturating_sub(total_gap_width)
        .checked_div(columns as u16)
        .unwrap_or(area.width)
        .max(1);
    let total_gap_height = CARD_GAP.saturating_mul(rows.saturating_sub(1) as u16);
    let card_height = area
        .height
        .saturating_sub(total_gap_height)
        .checked_div(rows as u16)
        .unwrap_or(area.height)
        .max(1);

    let mut cards = Vec::with_capacity(item_count);
    for index in 0..item_count {
        let col = index % columns;
        let row = index / columns;
        cards.push(Rect::new(
            area.x + col as u16 * (card_width + CARD_GAP),
            area.y + row as u16 * (card_height + CARD_GAP),
            card_width,
            card_height,
        ));
    }

    GridLayout {
        columns,
        rows,
        cards,
    }
}

fn render_centered_message(frame: &mut Frame<'_>, area: Rect, message: &str) {
    let paragraph = Paragraph::new(message)
        .style(Style::default().fg(Color::Gray))
        .alignment(ratatui::layout::Alignment::Center)
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, area);
}

fn truncate(value: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }

    let mut output = String::new();
    for ch in value.chars().take(max_width) {
        output.push(ch);
    }
    output
}

fn truncate_ansi(value: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }

    let mut output = String::new();
    let mut visible_width = 0;
    let mut chars = value.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' && chars.peek() == Some(&'[') {
            output.push(ch);
            output.push(chars.next().unwrap());
            for next in chars.by_ref() {
                output.push(next);
                if ('@'..='~').contains(&next) {
                    break;
                }
            }
            continue;
        }

        if visible_width == max_width {
            break;
        }

        output.push(ch);
        visible_width += 1;
    }

    output
}

fn ansi_to_line(value: &str) -> Line<'static> {
    let mut spans = Vec::new();
    let mut buffer = String::new();
    let mut style = Style::default();
    let mut chars = value.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            let mut sequence = String::new();
            let mut is_sgr = false;
            for next in chars.by_ref() {
                if next == 'm' {
                    is_sgr = true;
                    break;
                }
                if ('@'..='~').contains(&next) {
                    break;
                }
                sequence.push(next);
            }

            if is_sgr {
                flush_span(&mut spans, &mut buffer, style);
                apply_sgr(&sequence, &mut style);
            }
        } else {
            buffer.push(ch);
        }
    }

    flush_span(&mut spans, &mut buffer, style);
    Line::from(spans)
}

fn flush_span(spans: &mut Vec<Span<'static>>, buffer: &mut String, style: Style) {
    if !buffer.is_empty() {
        spans.push(Span::styled(std::mem::take(buffer), style));
    }
}

fn apply_sgr(sequence: &str, style: &mut Style) {
    let params: Vec<u16> = if sequence.is_empty() {
        vec![0]
    } else {
        sequence
            .split(';')
            .map(|part| part.parse::<u16>().unwrap_or(0))
            .collect()
    };
    let mut index = 0;

    while index < params.len() {
        match params[index] {
            0 => *style = Style::default(),
            1 => *style = style.add_modifier(Modifier::BOLD),
            3 => *style = style.add_modifier(Modifier::ITALIC),
            4 => *style = style.add_modifier(Modifier::UNDERLINED),
            22 => *style = style.remove_modifier(Modifier::BOLD),
            23 => *style = style.remove_modifier(Modifier::ITALIC),
            24 => *style = style.remove_modifier(Modifier::UNDERLINED),
            30..=37 => *style = style.fg(ansi_color(params[index] - 30, false)),
            39 => style.fg = None,
            40..=47 => *style = style.bg(ansi_color(params[index] - 40, false)),
            49 => style.bg = None,
            90..=97 => *style = style.fg(ansi_color(params[index] - 90, true)),
            100..=107 => *style = style.bg(ansi_color(params[index] - 100, true)),
            38 | 48 => {
                let is_fg = params[index] == 38;
                if params.get(index + 1) == Some(&5) {
                    if let Some(color) = params
                        .get(index + 2)
                        .and_then(|value| u8::try_from(*value).ok())
                    {
                        if is_fg {
                            *style = style.fg(Color::Indexed(color));
                        } else {
                            *style = style.bg(Color::Indexed(color));
                        }
                    }
                    index += 2;
                } else if params.get(index + 1) == Some(&2) {
                    let rgb = params.get(index + 2..index + 5).and_then(|values| {
                        values
                            .iter()
                            .map(|value| u8::try_from(*value).ok())
                            .collect::<Option<Vec<_>>>()
                    });
                    if let Some(rgb) = rgb {
                        let color = Color::Rgb(rgb[0], rgb[1], rgb[2]);
                        if is_fg {
                            *style = style.fg(color);
                        } else {
                            *style = style.bg(color);
                        }
                    }
                    index += 4;
                }
            }
            _ => {}
        }
        index += 1;
    }
}

fn ansi_color(index: u16, bright: bool) -> Color {
    match (index, bright) {
        (0, false) => Color::Black,
        (1, false) => Color::Red,
        (2, false) => Color::Green,
        (3, false) => Color::Yellow,
        (4, false) => Color::Blue,
        (5, false) => Color::Magenta,
        (6, false) => Color::Cyan,
        (7, false) => Color::Gray,
        (0, true) => Color::DarkGray,
        (1, true) => Color::LightRed,
        (2, true) => Color::LightGreen,
        (3, true) => Color::LightYellow,
        (4, true) => Color::LightBlue,
        (5, true) => Color::LightMagenta,
        (6, true) => Color::LightCyan,
        (7, true) => Color::White,
        _ => Color::Reset,
    }
}

#[cfg(test)]
mod tests {
    use ratatui::layout::Rect;

    use super::*;

    #[test]
    fn grid_uses_as_many_min_width_columns_as_fit() {
        let grid = calculate_grid(Rect::new(0, 0, 100, 30), 6, MIN_CARD_WIDTH, None);

        assert_eq!(grid.columns, 3);
        assert_eq!(grid.rows, 2);
        assert_eq!(grid.cards.len(), 6);
        assert!(grid.cards[0].width >= MIN_CARD_WIDTH);
    }

    #[test]
    fn grid_always_has_one_column_for_narrow_terminals() {
        let grid = calculate_grid(Rect::new(0, 0, 20, 30), 2, MIN_CARD_WIDTH, None);

        assert_eq!(grid.columns, 1);
        assert_eq!(grid.rows, 2);
        assert_eq!(grid.cards.len(), 2);
    }

    #[test]
    fn custom_min_card_width_makes_automatic_cards_larger() {
        let grid = calculate_grid(Rect::new(0, 0, 100, 30), 6, 50, None);

        assert_eq!(grid.columns, 1);
        assert_eq!(grid.cards[0].width, 100);
    }

    #[test]
    fn forced_columns_override_automatic_width_calculation() {
        let grid = calculate_grid(Rect::new(0, 0, 100, 30), 6, 50, Some(3));

        assert_eq!(grid.columns, 3);
        assert_eq!(grid.rows, 2);
    }

    #[test]
    fn ansi_foreground_colors_become_styled_spans() {
        let line = ansi_to_line("plain \u{1b}[31mred\u{1b}[0m done");

        assert_eq!(line.spans.len(), 3);
        assert_eq!(line.spans[0].content, "plain ");
        assert_eq!(line.spans[0].style.fg, None);
        assert_eq!(line.spans[1].content, "red");
        assert_eq!(line.spans[1].style.fg, Some(Color::Red));
        assert_eq!(line.spans[2].content, " done");
        assert_eq!(line.spans[2].style.fg, None);
    }

    #[test]
    fn ansi_truncation_counts_only_visible_characters() {
        let truncated = truncate_ansi("\u{1b}[31mred\u{1b}[0m plain", 5);

        assert_eq!(truncated, "\u{1b}[31mred\u{1b}[0m p");
    }
}

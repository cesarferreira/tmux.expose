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

/// Colors used to highlight session cards by state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CardColors {
    /// The currently selected card (title + border).
    pub selected: Color,
    /// The session you are currently attached to (title + border).
    pub attached: Color,
    /// Every other card (title only; the border stays dimmed).
    pub inactive: Color,
}

impl Default for CardColors {
    fn default() -> Self {
        Self {
            selected: Color::Yellow,
            attached: Color::Green,
            inactive: Color::White,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GridLayout {
    pub columns: usize,
    pub rows: usize,
    pub cards: Vec<Rect>,
}

pub fn render(
    frame: &mut Frame<'_>,
    app: &App,
    colors: CardColors,
    min_card_width: Option<u16>,
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
    } else if app.visible_session_count() == 0 {
        render_centered_message(frame, chunks[0], "No matching sessions");
    } else {
        render_grid(
            frame,
            app,
            colors,
            chunks[0],
            min_card_width,
            forced_columns,
        );
    }

    let footer = Paragraph::new(footer_hint_line(app.search_text(), app.vim_keys));
    frame.render_widget(footer, chunks[1]);
}

pub fn render_grid(
    frame: &mut Frame<'_>,
    app: &App,
    colors: CardColors,
    area: Rect,
    min_card_width: Option<u16>,
    forced_columns: Option<usize>,
) {
    let sessions = app.visible_sessions();
    let grid = calculate_grid(area, sessions.len(), min_card_width, forced_columns);

    for (index, card_area) in grid.cards.iter().enumerate() {
        if let Some(session) = sessions.get(index) {
            let current_attached = session.attached
                && app.current_session_name.as_deref() == Some(session.name.as_str());
            render_card(
                frame,
                session,
                index == app.selected_index,
                current_attached,
                colors,
                *card_area,
            );
        }
    }
}

pub fn render_card(
    frame: &mut Frame<'_>,
    session: &Session,
    selected: bool,
    current_attached: bool,
    colors: CardColors,
    area: Rect,
) {
    let title = format!(
        " {} ",
        truncate(&session.name, area.width.saturating_sub(12) as usize)
    );
    let block = Block::default()
        .title(Span::styled(
            title,
            card_title_style(selected, current_attached, colors),
        ))
        .title_bottom(session_status_span(session.attached))
        .borders(Borders::ALL)
        .border_type(if selected {
            BorderType::Double
        } else {
            BorderType::Plain
        })
        .border_style(card_border_style(selected, current_attached, colors));

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

fn card_title_style(selected: bool, current_attached: bool, colors: CardColors) -> Style {
    if selected {
        Style::default()
            .fg(colors.selected)
            .add_modifier(Modifier::BOLD)
    } else if current_attached {
        Style::default()
            .fg(colors.attached)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(colors.inactive)
            .add_modifier(Modifier::BOLD)
    }
}

fn card_border_style(selected: bool, current_attached: bool, colors: CardColors) -> Style {
    if selected {
        Style::default()
            .fg(colors.selected)
            .add_modifier(Modifier::BOLD)
    } else if current_attached {
        Style::default().fg(colors.attached)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}

fn session_status_span(attached: bool) -> Span<'static> {
    if attached {
        Span::styled(
            " attached ",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(" detached ", Style::default().fg(Color::DarkGray))
    }
}

fn footer_hint_line(search_query: Option<&str>, vim_keys: bool) -> Line<'static> {
    match (vim_keys, search_query) {
        // Vim SEARCH mode: typing filters, Esc returns to NORMAL.
        (true, Some(query)) => Line::from(vec![
            Span::styled(format!("Search: {query}"), Style::default().fg(Color::Cyan)),
            hint_text(" · type to filter · "),
            hint_key("Backspace"),
            hint_text(" to edit · "),
            hint_key("Enter"),
            hint_text(" to switch · "),
            hint_key("Esc"),
            hint_text(" for normal"),
        ]),
        // Vim NORMAL mode: hjkl/arrows move, `/` searches.
        (true, None) => Line::from(vec![
            hint_key("hjkl"),
            hint_text(" to move · "),
            hint_key("/"),
            hint_text(" to search · "),
            hint_key("Enter"),
            hint_text(" to switch · "),
            hint_key("q/Esc"),
            hint_text(" to quit"),
        ]),
        (false, Some(query)) => Line::from(vec![
            Span::styled(format!("Search: {query}"), Style::default().fg(Color::Cyan)),
            hint_text(" · type to filter · "),
            hint_key("Backspace"),
            hint_text(" to edit · "),
            hint_key("↑/↓/←/→"),
            hint_text(" to move · "),
            hint_key("Enter"),
            hint_text(" to switch · "),
            hint_key("Esc"),
            hint_text(" to clear"),
        ]),
        (false, None) => Line::from(vec![
            hint_text("type to filter · "),
            hint_key("↑/↓/←/→"),
            hint_text(" to move · "),
            hint_key("Enter"),
            hint_text(" to switch · "),
            hint_key("Esc/Ctrl-C"),
            hint_text(" to quit"),
        ]),
    }
}

fn hint_key(value: &'static str) -> Span<'static> {
    Span::styled(
        value,
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )
}

fn hint_text(value: &'static str) -> Span<'static> {
    Span::styled(value, Style::default().fg(Color::DarkGray))
}

pub fn calculate_grid(
    area: Rect,
    item_count: usize,
    min_card_width: Option<u16>,
    forced_columns: Option<usize>,
) -> GridLayout {
    if item_count == 0 || area.width == 0 || area.height == 0 {
        return GridLayout {
            columns: 1,
            rows: 0,
            cards: Vec::new(),
        };
    }

    let columns = forced_columns
        .unwrap_or_else(|| calculate_automatic_columns(area, item_count, min_card_width))
        .min(item_count)
        .min(u16::MAX as usize)
        .max(1);
    let rows = item_count.div_ceil(columns);
    let total_gap_width = CARD_GAP.saturating_mul(columns.saturating_sub(1) as u16);
    let usable_width = area
        .width
        .saturating_sub(total_gap_width)
        .max(columns as u16);
    let base_card_width = usable_width / columns as u16;
    let extra_width = usable_width % columns as u16;
    let total_gap_height = CARD_GAP.saturating_mul(rows.saturating_sub(1) as u16);
    let usable_height = area
        .height
        .saturating_sub(total_gap_height)
        .max(rows as u16);
    let base_card_height = usable_height / rows as u16;
    let extra_height = usable_height % rows as u16;

    let mut cards = Vec::with_capacity(item_count);
    for index in 0..item_count {
        let col = index % columns;
        let row = index / columns;
        let card_width = base_card_width + u16::from((col as u16) < extra_width);
        let card_height = base_card_height + u16::from((row as u16) < extra_height);
        let x_offset = (0..col)
            .map(|previous_col| base_card_width + u16::from((previous_col as u16) < extra_width))
            .sum::<u16>()
            + CARD_GAP.saturating_mul(col as u16);
        let y_offset = (0..row)
            .map(|previous_row| base_card_height + u16::from((previous_row as u16) < extra_height))
            .sum::<u16>()
            + CARD_GAP.saturating_mul(row as u16);
        cards.push(Rect::new(
            area.x + x_offset,
            area.y + y_offset,
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

fn calculate_automatic_columns(
    area: Rect,
    item_count: usize,
    min_card_width: Option<u16>,
) -> usize {
    if let Some(min_card_width) = min_card_width {
        return (area.width.saturating_add(CARD_GAP)
            / min_card_width.max(1).saturating_add(CARD_GAP))
        .max(1) as usize;
    }

    (1..=item_count.min(u16::MAX as usize))
        .min_by_key(|columns| {
            let rows = item_count.div_ceil(*columns);
            let empty_slots = columns.saturating_mul(rows).saturating_sub(item_count);
            let single_axis_penalty = usize::from(item_count > 2 && (*columns == 1 || rows == 1));
            let card_width = area
                .width
                .saturating_sub(CARD_GAP.saturating_mul(columns.saturating_sub(1) as u16))
                .checked_div(*columns as u16)
                .unwrap_or(area.width);
            let card_height = area
                .height
                .saturating_sub(CARD_GAP.saturating_mul(rows.saturating_sub(1) as u16))
                .checked_div(rows as u16)
                .unwrap_or(area.height);

            let aspect_penalty = u32::from(card_width)
                .saturating_mul(10)
                .abs_diff(u32::from(card_height).saturating_mul(16));
            let area = u32::from(card_width) * u32::from(card_height);
            (
                single_axis_penalty,
                empty_slots,
                aspect_penalty,
                std::cmp::Reverse(area),
            )
        })
        .unwrap_or(1)
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
    fn grid_fits_default_cards_to_available_screen_space() {
        let grid = calculate_grid(Rect::new(0, 0, 100, 30), 8, None, None);

        assert_eq!(grid.columns, 4);
        assert_eq!(grid.rows, 2);
        assert_eq!(grid.cards.len(), 8);
        assert_eq!(grid.cards[0].width, 24);
        assert_eq!(grid.cards[0].height, 14);
        assert_eq!(grid.cards[3].x + grid.cards[3].width, 100);
    }

    #[test]
    fn grid_keeps_wide_screens_balanced_by_default() {
        let grid = calculate_grid(Rect::new(0, 0, 240, 60), 9, None, None);

        assert_eq!(grid.columns, 3);
        assert_eq!(grid.rows, 3);
        assert!(grid.cards[0].width > grid.cards[0].height);
    }

    #[test]
    fn grid_prefers_complete_rows_when_space_is_available() {
        let grid = calculate_grid(Rect::new(0, 0, 240, 60), 6, None, None);

        assert_eq!(grid.columns, 3);
        assert_eq!(grid.rows, 2);
        assert_eq!(grid.cards.len(), 6);
        assert_eq!(grid.cards[5].x + grid.cards[5].width, 240);
    }

    #[test]
    fn grid_prefers_fewer_empty_slots_when_rows_cannot_be_complete() {
        let grid = calculate_grid(Rect::new(0, 0, 240, 60), 7, None, None);

        assert_eq!(grid.columns, 4);
        assert_eq!(grid.rows, 2);
        assert_eq!(grid.cards.len(), 7);
    }

    #[test]
    fn thumbnail_width_uses_as_many_min_width_columns_as_fit() {
        let grid = calculate_grid(Rect::new(0, 0, 100, 30), 6, Some(MIN_CARD_WIDTH), None);

        assert_eq!(grid.columns, 3);
        assert_eq!(grid.rows, 2);
        assert_eq!(grid.cards.len(), 6);
        assert!(grid.cards[0].width >= MIN_CARD_WIDTH);
    }

    #[test]
    fn grid_always_has_one_column_for_narrow_terminals() {
        let grid = calculate_grid(Rect::new(0, 0, 20, 30), 2, None, None);

        assert_eq!(grid.columns, 1);
        assert_eq!(grid.rows, 2);
        assert_eq!(grid.cards.len(), 2);
    }

    #[test]
    fn custom_min_card_width_makes_automatic_cards_larger() {
        let grid = calculate_grid(Rect::new(0, 0, 100, 30), 6, Some(50), None);

        assert_eq!(grid.columns, 1);
        assert_eq!(grid.cards[0].width, 100);
    }

    #[test]
    fn forced_columns_override_automatic_width_calculation() {
        let grid = calculate_grid(Rect::new(0, 0, 100, 30), 6, Some(50), Some(3));

        assert_eq!(grid.columns, 3);
        assert_eq!(grid.rows, 2);
    }

    #[test]
    fn attached_status_is_highlighted() {
        let span = session_status_span(true);

        assert_eq!(span.content, " attached ");
        assert_eq!(span.style.fg, Some(Color::Green));
        assert!(span.style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn detached_status_stays_muted() {
        let span = session_status_span(false);

        assert_eq!(span.content, " detached ");
        assert_eq!(span.style.fg, Some(Color::DarkGray));
        assert!(!span.style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn selected_session_title_is_yellow_and_bold() {
        let style = card_title_style(true, false, CardColors::default());

        assert_eq!(style.fg, Some(Color::Yellow));
        assert!(style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn current_attached_session_title_is_green_and_bold() {
        let style = card_title_style(false, true, CardColors::default());

        assert_eq!(style.fg, Some(Color::Green));
        assert!(style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn other_session_titles_are_white_and_bold() {
        let style = card_title_style(false, false, CardColors::default());

        assert_eq!(style.fg, Some(Color::White));
        assert!(style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn current_attached_session_border_is_green_when_not_selected() {
        let style = card_border_style(false, true, CardColors::default());

        assert_eq!(style.fg, Some(Color::Green));
        assert!(!style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn selected_session_border_stays_yellow_and_bold() {
        let style = card_border_style(true, true, CardColors::default());

        assert_eq!(style.fg, Some(Color::Yellow));
        assert!(style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn custom_colors_override_card_styles() {
        let colors = CardColors {
            selected: Color::Magenta,
            attached: Color::Blue,
            inactive: Color::Cyan,
        };

        assert_eq!(
            card_title_style(true, false, colors).fg,
            Some(Color::Magenta)
        );
        assert_eq!(
            card_border_style(true, false, colors).fg,
            Some(Color::Magenta)
        );
        assert_eq!(card_title_style(false, true, colors).fg, Some(Color::Blue));
        assert_eq!(card_border_style(false, true, colors).fg, Some(Color::Blue));
        assert_eq!(card_title_style(false, false, colors).fg, Some(Color::Cyan));
        // Unselected border stays dimmed regardless of the configured color.
        assert_eq!(
            card_border_style(false, false, colors).fg,
            Some(Color::DarkGray)
        );
    }

    #[test]
    fn footer_highlights_shortcuts_only() {
        let line = footer_hint_line(None, false);

        let shortcut_spans: Vec<&Span<'_>> = line
            .spans
            .iter()
            .filter(|span| span.style.fg == Some(Color::Yellow))
            .collect();
        let shortcut_text: Vec<&str> = shortcut_spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();

        assert_eq!(shortcut_text, vec!["↑/↓/←/→", "Enter", "Esc/Ctrl-C"]);
        assert!(
            shortcut_spans
                .iter()
                .all(|span| span.style.add_modifier.contains(Modifier::BOLD))
        );
        assert!(
            line.spans
                .iter()
                .filter(|span| span.style.fg != Some(Color::Yellow))
                .all(|span| span.style.fg == Some(Color::DarkGray))
        );
    }

    #[test]
    fn search_footer_highlights_search_shortcuts() {
        let line = footer_hint_line(Some("api"), false);

        let shortcut_text: Vec<&str> = line
            .spans
            .iter()
            .filter(|span| span.style.fg == Some(Color::Yellow))
            .map(|span| span.content.as_ref())
            .collect();

        assert_eq!(shortcut_text, vec!["Backspace", "↑/↓/←/→", "Enter", "Esc"]);
        assert_eq!(line.spans[0].content, "Search: api");
        assert_eq!(line.spans[0].style.fg, Some(Color::Cyan));
    }

    #[test]
    fn vim_normal_footer_highlights_vim_shortcuts() {
        let line = footer_hint_line(None, true);

        let shortcut_text: Vec<&str> = line
            .spans
            .iter()
            .filter(|span| span.style.fg == Some(Color::Yellow))
            .map(|span| span.content.as_ref())
            .collect();

        assert_eq!(shortcut_text, vec!["hjkl", "/", "Enter", "q/Esc"]);
    }

    #[test]
    fn vim_search_footer_offers_return_to_normal() {
        let line = footer_hint_line(Some("api"), true);

        let shortcut_text: Vec<&str> = line
            .spans
            .iter()
            .filter(|span| span.style.fg == Some(Color::Yellow))
            .map(|span| span.content.as_ref())
            .collect();

        assert_eq!(shortcut_text, vec!["Backspace", "Enter", "Esc"]);
        assert_eq!(line.spans[0].content, "Search: api");
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

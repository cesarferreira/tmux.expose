use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use crate::{model::App, ui};

pub fn handle_key(app: &mut App, key: KeyEvent, columns: usize) {
    if key.code == KeyCode::Char('c') && key.modifiers == KeyModifiers::CONTROL {
        app.should_quit = true;
        return;
    }

    if app.is_searching() {
        handle_search_key(app, key, columns);
        return;
    }

    match (key.code, key.modifiers) {
        (KeyCode::Esc, _) => app.should_quit = true,
        (KeyCode::Enter, _) => app.should_switch = true,
        (KeyCode::Left, _) => move_left(app, columns),
        (KeyCode::Right, _) => move_right(app, columns),
        (KeyCode::Up, _) => app.move_up(columns),
        (KeyCode::Down, _) => app.move_down(columns),
        (KeyCode::Char(ch), KeyModifiers::NONE | KeyModifiers::SHIFT) => push_filter_char(app, ch),
        _ => {}
    }
}

pub fn handle_mouse(
    app: &mut App,
    mouse: MouseEvent,
    grid_area: Rect,
    min_card_width: Option<u16>,
    forced_columns: Option<usize>,
) {
    if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
        return;
    }

    let grid = ui::calculate_grid(
        grid_area,
        app.visible_session_count(),
        min_card_width,
        forced_columns,
    );
    if let Some(index) = grid
        .cards
        .iter()
        .position(|card| contains(*card, mouse.column, mouse.row))
    {
        app.selected_index = index;
        app.should_switch = true;
    }
}

fn contains(area: Rect, x: u16, y: u16) -> bool {
    x >= area.x
        && x < area.x.saturating_add(area.width)
        && y >= area.y
        && y < area.y.saturating_add(area.height)
}

fn handle_search_key(app: &mut App, key: KeyEvent, columns: usize) {
    match (key.code, key.modifiers) {
        (KeyCode::Esc, _) if app.search_text().is_some_and(str::is_empty) => app.should_quit = true,
        (KeyCode::Esc, _) => app.clear_search(),
        (KeyCode::Enter, _) => app.should_switch = true,
        (KeyCode::Backspace, _) => app.pop_search_char(),
        (KeyCode::Left, _) => move_left(app, columns),
        (KeyCode::Right, _) => move_right(app, columns),
        (KeyCode::Up, _) => app.move_up(columns),
        (KeyCode::Down, _) => app.move_down(columns),
        (KeyCode::Char(ch), KeyModifiers::NONE | KeyModifiers::SHIFT) => push_filter_char(app, ch),
        _ => {}
    }
}

fn push_filter_char(app: &mut App, ch: char) {
    if !app.is_searching() {
        app.start_search();
    }
    app.push_search_char(ch);
}

fn move_left(app: &mut App, columns: usize) {
    let columns = columns.max(1);
    if !app.selected_index.is_multiple_of(columns) {
        app.move_left();
    }
}

fn move_right(app: &mut App, columns: usize) {
    let columns = columns.max(1);
    if app.selected_index % columns != columns - 1 {
        app.move_right();
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{
        KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    };
    use ratatui::layout::Rect;

    use super::*;
    use crate::model::{App, Session};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn session(name: &str) -> Session {
        Session {
            id: format!("${name}"),
            name: name.to_string(),
            attached: false,
            window_count: 1,
            current_window: None,
            last_activity: None,
            preview: Vec::new(),
            preview_error: None,
        }
    }

    #[test]
    fn arrow_keys_move_selection() {
        let mut app = App::new(vec![session("one"), session("two"), session("three")], None);

        handle_key(&mut app, key(KeyCode::Right), 2);
        assert_eq!(app.selected_index, 1);

        handle_key(&mut app, key(KeyCode::Down), 2);
        assert_eq!(app.selected_index, 2);

        handle_key(&mut app, key(KeyCode::Left), 2);
        assert_eq!(app.selected_index, 2);
    }

    #[test]
    fn hjkl_filter_instead_of_moving() {
        let mut app = App::new(vec![session("one"), session("two"), session("three")], None);

        handle_key(&mut app, key(KeyCode::Char('h')), 2);

        assert_eq!(app.selected_index, 0);
        assert_eq!(app.search_text(), Some("h"));
    }

    #[test]
    fn q_filters_instead_of_quitting() {
        let mut app = App::new(vec![session("queue")], None);

        handle_key(&mut app, key(KeyCode::Char('q')), 1);

        assert_eq!(app.search_text(), Some("q"));
        assert!(!app.should_quit);
    }

    #[test]
    fn enter_marks_app_for_switch() {
        let mut app = App::new(vec![session("one")], None);

        handle_key(&mut app, key(KeyCode::Enter), 1);

        assert!(app.should_switch);
    }

    #[test]
    fn ctrl_c_marks_app_for_exit() {
        let mut app = App::new(vec![session("one")], None);

        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
            1,
        );

        assert!(app.should_quit);
    }

    #[test]
    fn alt_e_does_not_quit() {
        let mut app = App::new(vec![session("one")], None);

        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('e'), KeyModifiers::ALT),
            1,
        );

        assert!(!app.should_quit);
        assert!(!app.should_switch);
    }

    #[test]
    fn left_click_on_session_marks_it_for_switch() {
        let mut app = App::new(vec![session("one"), session("two"), session("three")], None);
        let mouse = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 40,
            row: 1,
            modifiers: KeyModifiers::NONE,
        };

        handle_mouse(&mut app, mouse, Rect::new(0, 0, 100, 20), None, Some(3));

        assert_eq!(app.selected_index, 1);
        assert!(app.should_switch);
        assert!(!app.should_quit);
    }

    #[test]
    fn characters_start_filtering_without_slash() {
        let mut app = App::new(
            vec![session("backend"), session("frontend"), session("database")],
            None,
        );

        handle_key(&mut app, key(KeyCode::Char('f')), 1);

        assert!(app.is_searching());
        assert_eq!(app.search_text(), Some("f"));
        assert_eq!(app.visible_session_count(), 1);
        assert_eq!(app.selected_session().unwrap().name, "frontend");
    }

    #[test]
    fn slash_filters_instead_of_starting_empty_search() {
        let mut app = App::new(vec![session("docs/api")], None);

        handle_key(&mut app, key(KeyCode::Char('/')), 1);

        assert_eq!(app.search_text(), Some("/"));
        assert_eq!(app.visible_session_count(), 1);
        assert_eq!(app.selected_session().unwrap().name, "docs/api");
    }

    #[test]
    fn characters_continue_filtering_while_searching() {
        let mut app = App::new(
            vec![session("backend"), session("frontend"), session("database")],
            None,
        );

        handle_key(&mut app, key(KeyCode::Char('f')), 1);
        handle_key(&mut app, key(KeyCode::Char('r')), 1);

        assert_eq!(app.search_text(), Some("fr"));
        assert_eq!(app.visible_session_count(), 1);
        assert_eq!(app.selected_session().unwrap().name, "frontend");
    }

    #[test]
    fn esc_clears_search_before_quitting() {
        let mut app = App::new(vec![session("one")], None);

        handle_key(&mut app, key(KeyCode::Char('o')), 1);
        handle_key(&mut app, key(KeyCode::Esc), 1);

        assert!(!app.is_searching());
        assert!(!app.should_quit);
    }

    #[test]
    fn esc_quits_when_search_query_is_empty() {
        let mut app = App::new(vec![session("one")], None);

        handle_key(&mut app, key(KeyCode::Char('o')), 1);
        handle_key(&mut app, key(KeyCode::Backspace), 1);
        handle_key(&mut app, key(KeyCode::Esc), 1);

        assert!(app.should_quit);
    }

    #[test]
    fn backspace_edits_search_query() {
        let mut app = App::new(vec![session("frontend")], None);

        handle_key(&mut app, key(KeyCode::Char('f')), 1);
        handle_key(&mut app, key(KeyCode::Backspace), 1);

        assert_eq!(app.search_text(), Some(""));
    }

    #[test]
    fn horizontal_navigation_clamps_at_row_edges() {
        let mut app = App::new(
            vec![
                session("one"),
                session("two"),
                session("three"),
                session("four"),
            ],
            None,
        );
        app.selected_index = 2;

        handle_key(&mut app, key(KeyCode::Right), 3);
        assert_eq!(app.selected_index, 2);

        app.selected_index = 3;
        handle_key(&mut app, key(KeyCode::Left), 3);
        assert_eq!(app.selected_index, 3);
    }
}

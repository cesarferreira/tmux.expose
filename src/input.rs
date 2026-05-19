use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::model::App;

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
        (KeyCode::Esc, _) | (KeyCode::Char('q'), _) => app.should_quit = true,
        (KeyCode::Enter, _) => app.should_switch = true,
        (KeyCode::Char('/'), KeyModifiers::NONE) => app.start_search(),
        (KeyCode::Left, _) | (KeyCode::Char('h'), _) => move_left(app, columns),
        (KeyCode::Right, _) | (KeyCode::Char('l'), _) => move_right(app, columns),
        (KeyCode::Up, _) | (KeyCode::Char('k'), _) => app.move_up(columns),
        (KeyCode::Down, _) | (KeyCode::Char('j'), _) => app.move_down(columns),
        _ => {}
    }
}

fn handle_search_key(app: &mut App, key: KeyEvent, columns: usize) {
    match (key.code, key.modifiers) {
        (KeyCode::Esc, _) => app.clear_search(),
        (KeyCode::Enter, _) => app.should_switch = true,
        (KeyCode::Backspace, _) => app.pop_search_char(),
        (KeyCode::Left, _) => move_left(app, columns),
        (KeyCode::Right, _) => move_right(app, columns),
        (KeyCode::Up, _) => app.move_up(columns),
        (KeyCode::Down, _) => app.move_down(columns),
        (KeyCode::Char(ch), KeyModifiers::NONE | KeyModifiers::SHIFT) => app.push_search_char(ch),
        _ => {}
    }
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
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

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
    fn hjkl_keys_move_selection() {
        let mut app = App::new(vec![session("one"), session("two"), session("three")], None);

        handle_key(&mut app, key(KeyCode::Char('l')), 2);
        handle_key(&mut app, key(KeyCode::Char('j')), 2);
        assert_eq!(app.selected_index, 2);

        handle_key(&mut app, key(KeyCode::Char('h')), 2);
        assert_eq!(app.selected_index, 2);
    }

    #[test]
    fn quit_keys_mark_app_for_exit() {
        let mut app = App::new(vec![session("one")], None);

        handle_key(&mut app, key(KeyCode::Char('q')), 1);

        assert!(app.should_quit);
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
    fn slash_enters_search_mode() {
        let mut app = App::new(vec![session("one")], None);

        handle_key(&mut app, key(KeyCode::Char('/')), 1);

        assert!(app.is_searching());
        assert_eq!(app.search_text(), Some(""));
    }

    #[test]
    fn characters_filter_sessions_while_searching() {
        let mut app = App::new(
            vec![session("backend"), session("frontend"), session("database")],
            None,
        );

        handle_key(&mut app, key(KeyCode::Char('/')), 1);
        handle_key(&mut app, key(KeyCode::Char('f')), 1);

        assert_eq!(app.search_text(), Some("f"));
        assert_eq!(app.visible_session_count(), 1);
        assert_eq!(app.selected_session().unwrap().name, "frontend");
    }

    #[test]
    fn esc_clears_search_before_quitting() {
        let mut app = App::new(vec![session("one")], None);

        handle_key(&mut app, key(KeyCode::Char('/')), 1);
        handle_key(&mut app, key(KeyCode::Esc), 1);

        assert!(!app.is_searching());
        assert!(!app.should_quit);
    }

    #[test]
    fn backspace_edits_search_query() {
        let mut app = App::new(vec![session("frontend")], None);

        handle_key(&mut app, key(KeyCode::Char('/')), 1);
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

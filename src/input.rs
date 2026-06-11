use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use crate::{model::App, ui};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ToggleKey {
    code: KeyCode,
    modifiers: KeyModifiers,
}

impl ToggleKey {
    pub fn from_tmux_key(value: &str) -> Option<Self> {
        let (mut modifiers, key) = if let Some(key) = value.strip_prefix("M-") {
            (KeyModifiers::ALT, key)
        } else if let Some(key) = value.strip_prefix("C-") {
            (KeyModifiers::CONTROL, key)
        } else {
            (KeyModifiers::NONE, value)
        };

        let code = match key {
            "Esc" => KeyCode::Esc,
            key if key.chars().count() == 1 => {
                let ch = key.chars().next()?;
                if ch.is_ascii_uppercase() {
                    modifiers.insert(KeyModifiers::SHIFT);
                }
                KeyCode::Char(ch)
            }
            _ => return None,
        };

        Some(Self { code, modifiers })
    }

    fn matches(self, key: KeyEvent) -> bool {
        self.code == key.code && self.modifiers == key.modifiers
    }
}

pub fn handle_key(app: &mut App, key: KeyEvent, columns: usize) {
    handle_key_with_toggle(app, key, columns, None);
}

pub fn handle_key_with_toggle(
    app: &mut App,
    key: KeyEvent,
    columns: usize,
    toggle_key: Option<ToggleKey>,
) {
    if key.code == KeyCode::Char('c') && key.modifiers == KeyModifiers::CONTROL {
        app.should_quit = true;
        return;
    }

    if toggle_key.is_some_and(|toggle_key| toggle_key.matches(key)) {
        app.should_quit = true;
        return;
    }

    if app.is_searching() {
        handle_search_key(app, key, columns);
        return;
    }

    if app.vim_keys {
        handle_vim_normal_key(app, key, columns);
        return;
    }

    handle_default_key(app, key, columns);
}

/// Default mode: typing any character immediately fuzzy-filters the list.
fn handle_default_key(app: &mut App, key: KeyEvent, columns: usize) {
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

/// Vim NORMAL mode: hjkl (and arrows) move the selection, `/` enters search.
fn handle_vim_normal_key(app: &mut App, key: KeyEvent, columns: usize) {
    match (key.code, key.modifiers) {
        (KeyCode::Esc, _) | (KeyCode::Char('q'), KeyModifiers::NONE) => app.should_quit = true,
        (KeyCode::Enter, _) => app.should_switch = true,
        (KeyCode::Char('/'), KeyModifiers::NONE) => app.start_search(),
        (KeyCode::Left, _) | (KeyCode::Char('h'), KeyModifiers::NONE) => move_left(app, columns),
        (KeyCode::Right, _) | (KeyCode::Char('l'), KeyModifiers::NONE) => move_right(app, columns),
        (KeyCode::Up, _) | (KeyCode::Char('k'), KeyModifiers::NONE) => app.move_up(columns),
        (KeyCode::Down, _) | (KeyCode::Char('j'), KeyModifiers::NONE) => app.move_down(columns),
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
        // In vim mode, Esc always returns to NORMAL rather than quitting.
        (KeyCode::Esc, _) if app.vim_keys => app.clear_search(),
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

    fn vim_app(names: &[&str]) -> App {
        let mut app = App::new(names.iter().map(|name| session(name)).collect(), None);
        app.vim_keys = true;
        app
    }

    #[test]
    fn vim_hjkl_move_selection_without_filtering() {
        let mut app = vim_app(&["one", "two", "three", "four"]);

        handle_key(&mut app, key(KeyCode::Char('l')), 2); // right
        assert_eq!(app.selected_index, 1);

        handle_key(&mut app, key(KeyCode::Char('j')), 2); // down
        assert_eq!(app.selected_index, 3);

        handle_key(&mut app, key(KeyCode::Char('h')), 2); // left
        assert_eq!(app.selected_index, 2);

        handle_key(&mut app, key(KeyCode::Char('k')), 2); // up
        assert_eq!(app.selected_index, 0);

        // hjkl must not leak into the search filter while navigating.
        assert!(!app.is_searching());
    }

    #[test]
    fn vim_arrows_still_move_selection() {
        let mut app = vim_app(&["one", "two", "three"]);

        handle_key(&mut app, key(KeyCode::Right), 2);
        assert_eq!(app.selected_index, 1);

        handle_key(&mut app, key(KeyCode::Down), 2);
        assert_eq!(app.selected_index, 2);
    }

    #[test]
    fn vim_slash_enters_search_mode() {
        let mut app = vim_app(&["one", "two"]);

        handle_key(&mut app, key(KeyCode::Char('/')), 1);

        assert!(app.is_searching());
        assert_eq!(app.search_text(), Some(""));
    }

    #[test]
    fn vim_typing_in_search_filters_including_hjkl() {
        let mut app = vim_app(&["backend", "frontend", "hjkl-box"]);

        handle_key(&mut app, key(KeyCode::Char('/')), 1);
        handle_key(&mut app, key(KeyCode::Char('h')), 1);

        assert_eq!(app.search_text(), Some("h"));
        assert_eq!(app.selected_session().unwrap().name, "hjkl-box");
    }

    #[test]
    fn vim_esc_in_search_returns_to_normal_without_quitting() {
        let mut app = vim_app(&["one", "two"]);

        handle_key(&mut app, key(KeyCode::Char('/')), 1);
        handle_key(&mut app, key(KeyCode::Char('o')), 1);
        handle_key(&mut app, key(KeyCode::Esc), 1);

        assert!(!app.is_searching());
        assert!(!app.should_quit);
    }

    #[test]
    fn vim_plain_letters_do_not_filter_in_normal_mode() {
        let mut app = vim_app(&["alpha", "beta"]);

        handle_key(&mut app, key(KeyCode::Char('a')), 1);

        assert!(!app.is_searching());
        assert_eq!(app.search_text(), None);
    }

    #[test]
    fn vim_q_quits_in_normal_mode() {
        let mut app = vim_app(&["queue"]);

        handle_key(&mut app, key(KeyCode::Char('q')), 1);

        assert!(app.should_quit);
        assert!(!app.is_searching());
    }

    #[test]
    fn vim_esc_quits_in_normal_mode() {
        let mut app = vim_app(&["one"]);

        handle_key(&mut app, key(KeyCode::Esc), 1);

        assert!(app.should_quit);
    }

    #[test]
    fn vim_enter_marks_app_for_switch() {
        let mut app = vim_app(&["one"]);

        handle_key(&mut app, key(KeyCode::Enter), 1);

        assert!(app.should_switch);
    }

    #[test]
    fn vim_ctrl_c_still_quits() {
        // Ctrl-C is handled before the mode dispatch, so it works in vim mode too.
        let mut app = vim_app(&["one"]);

        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
            1,
        );

        assert!(app.should_quit);
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
    fn alt_e_does_not_quit_without_configured_toggle_key() {
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
    fn configured_alt_e_marks_app_for_exit() {
        let mut app = App::new(vec![session("one")], None);
        let toggle_key = ToggleKey::from_tmux_key("M-e");

        handle_key_with_toggle(
            &mut app,
            KeyEvent::new(KeyCode::Char('e'), KeyModifiers::ALT),
            1,
            toggle_key,
        );

        assert!(app.should_quit);
        assert!(!app.should_switch);
    }

    #[test]
    fn configured_plain_key_marks_app_for_exit_while_searching() {
        let mut app = App::new(vec![session("one")], None);
        let toggle_key = ToggleKey::from_tmux_key("s");
        app.start_search();
        app.push_search_char('o');

        handle_key_with_toggle(
            &mut app,
            KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE),
            1,
            toggle_key,
        );

        assert!(app.should_quit);
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

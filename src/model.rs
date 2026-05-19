#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub id: String,
    pub name: String,
    pub attached: bool,
    pub window_count: u32,
    pub current_window: Option<String>,
    pub last_activity: Option<String>,
    pub preview: Vec<String>,
    pub preview_error: Option<String>,
}

#[derive(Debug)]
pub struct App {
    pub sessions: Vec<Session>,
    pub selected_index: usize,
    pub current_session_name: Option<String>,
    pub should_quit: bool,
    pub should_switch: bool,
    pub error: Option<String>,
    search_query: Option<String>,
}

impl App {
    pub fn new(sessions: Vec<Session>, current_session_name: Option<String>) -> Self {
        let selected_index = current_session_name
            .as_ref()
            .and_then(|name| sessions.iter().position(|session| &session.name == name))
            .unwrap_or(0);

        Self {
            sessions,
            selected_index,
            current_session_name,
            should_quit: false,
            should_switch: false,
            error: None,
            search_query: None,
        }
    }

    pub fn selected_session(&self) -> Option<&Session> {
        self.visible_sessions().get(self.selected_index).copied()
    }

    pub fn visible_sessions(&self) -> Vec<&Session> {
        match self.search_query.as_deref() {
            Some(query) => self
                .sessions
                .iter()
                .filter(|session| fuzzy_matches(&session.name, query))
                .collect(),
            None => self.sessions.iter().collect(),
        }
    }

    pub fn visible_session_count(&self) -> usize {
        self.visible_sessions().len()
    }

    pub fn start_search(&mut self) {
        self.search_query = Some(String::new());
        self.selected_index = 0;
    }

    pub fn push_search_char(&mut self, ch: char) {
        if let Some(query) = &mut self.search_query {
            query.push(ch);
            self.selected_index = 0;
        }
    }

    pub fn pop_search_char(&mut self) {
        if let Some(query) = &mut self.search_query {
            query.pop();
            self.selected_index = 0;
        }
    }

    pub fn clear_search(&mut self) {
        self.search_query = None;
        self.selected_index = 0;
    }

    pub fn is_searching(&self) -> bool {
        self.search_query.is_some()
    }

    pub fn search_text(&self) -> Option<&str> {
        self.search_query.as_deref()
    }

    pub fn replace_sessions(&mut self, sessions: Vec<Session>) {
        let selected_name = self.selected_session().map(|session| session.name.clone());
        self.sessions = sessions;

        if self.visible_session_count() == 0 {
            self.selected_index = 0;
            return;
        }

        self.selected_index = selected_name
            .and_then(|name| {
                self.visible_sessions()
                    .into_iter()
                    .position(|session| session.name == name)
            })
            .unwrap_or_else(|| self.selected_index.min(self.visible_session_count() - 1));
    }

    pub fn move_left(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    pub fn move_right(&mut self) {
        if self.selected_index + 1 < self.visible_session_count() {
            self.selected_index += 1;
        }
    }

    pub fn move_up(&mut self, columns: usize) {
        let columns = columns.max(1);
        if self.selected_index >= columns {
            self.selected_index -= columns;
        }
    }

    pub fn move_down(&mut self, columns: usize) {
        let columns = columns.max(1);
        let visible_count = self.visible_session_count();
        if visible_count == 0 {
            return;
        }

        let last_index = visible_count - 1;
        let current_row = self.selected_index / columns;
        let last_row = last_index / columns;
        if current_row < last_row {
            self.selected_index = self.selected_index.saturating_add(columns).min(last_index);
        }
    }
}

fn fuzzy_matches(name: &str, query: &str) -> bool {
    let query = query.to_lowercase();
    if query.is_empty() {
        return true;
    }

    let name = name.to_lowercase();
    let mut name_chars = name.chars();
    query
        .chars()
        .all(|query_ch| name_chars.any(|name_ch| name_ch == query_ch))
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn selects_current_session_when_present() {
        let app = App::new(
            vec![session("dev"), session("logs"), session("notes")],
            Some("logs".to_string()),
        );

        assert_eq!(app.selected_index, 1);
    }

    #[test]
    fn clamps_navigation_at_grid_edges() {
        let mut app = App::new(vec![session("one"), session("two"), session("three")], None);

        app.move_left();
        assert_eq!(app.selected_index, 0);

        app.move_right();
        app.move_right();
        app.move_right();
        assert_eq!(app.selected_index, 2);

        app.move_down(2);
        assert_eq!(app.selected_index, 2);

        app.move_up(2);
        assert_eq!(app.selected_index, 0);
    }

    #[test]
    fn preserves_selected_session_by_name_after_refresh() {
        let mut app = App::new(
            vec![session("dev"), session("logs"), session("notes")],
            None,
        );
        app.selected_index = 1;

        app.replace_sessions(vec![session("new"), session("logs"), session("dev")]);

        assert_eq!(app.selected_session().unwrap().name, "logs");
    }

    #[test]
    fn search_filters_sessions_by_fuzzy_name() {
        let mut app = App::new(
            vec![
                session("backend-api"),
                session("frontend"),
                session("database"),
            ],
            None,
        );

        app.start_search();
        app.push_search_char('b');
        app.push_search_char('a');

        let names: Vec<&str> = app
            .visible_sessions()
            .into_iter()
            .map(|session| session.name.as_str())
            .collect();
        assert_eq!(names, vec!["backend-api", "database"]);
    }

    #[test]
    fn selected_session_uses_filtered_selection() {
        let mut app = App::new(
            vec![session("backend"), session("frontend"), session("database")],
            None,
        );

        app.start_search();
        app.push_search_char('f');

        assert_eq!(app.selected_index, 0);
        assert_eq!(app.selected_session().unwrap().name, "frontend");
    }

    #[test]
    fn clearing_search_restores_all_sessions() {
        let mut app = App::new(vec![session("backend"), session("frontend")], None);

        app.start_search();
        app.push_search_char('f');
        app.clear_search();

        assert!(!app.is_searching());
        assert_eq!(app.visible_session_count(), 2);
    }

    #[test]
    fn up_from_first_row_keeps_selection_in_place() {
        let mut app = App::new(vec![session("one"), session("two"), session("three")], None);
        app.selected_index = 1;

        app.move_up(2);

        assert_eq!(app.selected_index, 1);
    }

    #[test]
    fn down_to_incomplete_row_selects_nearest_card() {
        let mut app = App::new(
            vec![
                session("one"),
                session("two"),
                session("three"),
                session("four"),
                session("five"),
            ],
            None,
        );
        app.selected_index = 2;

        app.move_down(3);

        assert_eq!(app.selected_index, 4);
    }
}

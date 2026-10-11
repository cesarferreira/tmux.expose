/// A Claude Code (or other agent) pane's status, as written by an external
/// hook into the tmux pane option `@agent_status`. Ordered least-to-most
/// severe so the derived `Ord` can be used to pick the worst value when
/// folding a session's panes down to one badge: `Attention` (blocked on
/// you) outranks `Waiting` (its turn just ended) outranks `Working` (still running).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AgentStatus {
    Working,
    Waiting,
    Attention,
}

impl AgentStatus {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "working" => Some(Self::Working),
            "waiting" => Some(Self::Waiting),
            "attention" => Some(Self::Attention),
            _ => None,
        }
    }
}

/// How many panes in a session sit at each agent status. Used to render a
/// count badge (e.g. "2 waiting, 1 working") instead of collapsing a
/// multi-agent session down to a single pane's state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AgentPaneCounts {
    pub working: u32,
    pub waiting: u32,
    pub attention: u32,
}

impl AgentPaneCounts {
    pub fn total(&self) -> u32 {
        self.working + self.waiting + self.attention
    }
}

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
    /// The worst-ranked status among this session's panes, or `None` if no
    /// pane has ever reported one.
    pub agent_status: Option<AgentStatus>,
    /// Unix timestamp (seconds) of the oldest pane at `agent_status`'s rank
    /// — i.e. how long the most-neglected pane has held that status.
    pub agent_status_since: Option<i64>,
    pub agent_pane_counts: AgentPaneCounts,
}

#[derive(Debug)]
pub struct App {
    pub sessions: Vec<Session>,
    pub selected_index: usize,
    pub current_session_name: Option<String>,
    pub should_quit: bool,
    pub should_switch: bool,
    pub error: Option<String>,
    /// When true, the picker uses modal vim navigation (hjkl to move, `/` to search).
    pub vim_keys: bool,
    search_query: Option<String>,
    /// True while actively typing a query. In vim mode this is distinct from
    /// `search_query` being set: Esc leaves editing (so hjkl moves again) but
    /// keeps `search_query` as an applied filter, matching how Telescope-style
    /// pickers commit a search into normal-mode browsing instead of discarding
    /// it. Default mode has no normal-mode navigation to return to, so the two
    /// always stay in lockstep there — see `is_searching`.
    editing_search: bool,
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
            vim_keys: false,
            search_query: None,
            editing_search: false,
        }
    }

    /// Moves the cursor to the top card when an agent there is blocked on or
    /// waiting for you. Meant to run after
    /// `sort_sessions_by_agent_status_with_current`, so the top card is the
    /// most urgent one other than the current session
    /// (which sorts last within its rank); otherwise the cursor stays on the
    /// current session. A merely `Working` agent doesn't need you yet, so it
    /// doesn't pull the cursor away.
    pub fn select_most_urgent_agent_session(&mut self) {
        let needs_you = self.sessions.first().is_some_and(|session| {
            matches!(
                session.agent_status,
                Some(AgentStatus::Attention | AgentStatus::Waiting)
            )
        });
        if needs_you {
            self.selected_index = 0;
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
        self.editing_search = true;
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
        self.editing_search = false;
        self.selected_index = 0;
    }

    /// Leaves text-entry but keeps `search_query` as an applied filter — vim
    /// mode's Esc-while-searching, so hjkl navigates the filtered results
    /// instead of discarding them.
    pub fn stop_editing_search(&mut self) {
        self.editing_search = false;
    }

    /// Whether keystrokes should be treated as search text-entry right now.
    /// Also drives the toggle-key/typeable-filter check in `input.rs`. In
    /// vim mode this is `false` while a filter is applied but not being
    /// edited; in default mode it always matches `search_query.is_some()`,
    /// since default mode has no normal-mode navigation to drop into.
    pub fn is_searching(&self) -> bool {
        self.editing_search
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

    pub fn replace_sessions_preserving_preview_for(
        &mut self,
        mut sessions: Vec<Session>,
        preserved_session_id: Option<&str>,
    ) {
        if let Some(preserved_session_id) = preserved_session_id
            && let Some(previous) = self
                .sessions
                .iter()
                .find(|session| session.id == preserved_session_id)
            && let Some(next) = sessions
                .iter_mut()
                .find(|session| session.id == preserved_session_id)
        {
            next.preview = previous.preview.clone();
            next.preview_error = previous.preview_error.clone();
        }

        self.replace_sessions(sessions);
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

/// Reorders sessions so the ones an agent is waiting on you for lead the
/// grid: `Attention` first, then `Waiting`, then `Working`, then sessions
/// with no agent at all. Within a rank, the longest-waiting session (oldest
/// `agent_status_since`) sorts first, so a neglected pane doesn't get
/// buried under one that only just finished. The current session sorts last
/// within its rank: you're already looking at it, so the other sessions at
/// that urgency are the ones worth surfacing. Stable, so untracked sessions
/// keep tmux's own ordering relative to each other.
pub fn sort_sessions_by_agent_status_with_current(
    sessions: &mut [Session],
    current_session_name: Option<&str>,
) {
    sessions.sort_by_key(|session| {
        let rank = match session.agent_status {
            Some(AgentStatus::Attention) => 3,
            Some(AgentStatus::Waiting) => 2,
            Some(AgentStatus::Working) => 1,
            None => 0,
        };
        let is_tracked_current =
            session.agent_status.is_some() && current_session_name == Some(session.name.as_str());
        (
            std::cmp::Reverse(rank),
            is_tracked_current,
            session.agent_status_since.unwrap_or(i64::MAX),
        )
    });
}

/// Same ordering as [`sort_sessions_by_agent_status_with_current`], without a
/// current-session tiebreak. Kept as its own public function (rather than
/// changing this signature to take an `Option<&str>`) so existing callers of
/// this library function keep compiling.
pub fn sort_sessions_by_agent_status(sessions: &mut [Session]) {
    sort_sessions_by_agent_status_with_current(sessions, None);
}

/// Picks the session `tmux-expose next` should jump to: the next one after
/// the current session in priority order among those whose agent is blocked
/// on or waiting for you, wrapping around. Cycling (rather than always taking
/// the most urgent non-current session) matters because a session you just
/// left usually stays `waiting` — "most urgent" would bounce between the two
/// oldest waiters and never reach a third. Returns `None` when nothing else
/// needs you.
pub fn next_urgent_session<'a>(
    sessions: &'a [Session],
    current_session_id: Option<&str>,
) -> Option<&'a Session> {
    let mut urgent: Vec<Session> = sessions
        .iter()
        .filter(|session| {
            matches!(
                session.agent_status,
                Some(AgentStatus::Attention | AgentStatus::Waiting)
            )
        })
        .cloned()
        .collect();
    // No current-session tiebreak here: the current session has to keep its
    // natural slot so "the one after it" walks the whole list.
    sort_sessions_by_agent_status(&mut urgent);

    let next = match urgent
        .iter()
        .position(|session| Some(session.id.as_str()) == current_session_id)
    {
        Some(index) => &urgent[(index + 1) % urgent.len()],
        None => urgent.first()?,
    };
    if Some(next.id.as_str()) == current_session_id {
        return None;
    }
    sessions.iter().find(|session| session.id == next.id)
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
            agent_status: None,
            agent_status_since: None,
            agent_pane_counts: AgentPaneCounts::default(),
        }
    }

    fn session_with_agent_status(
        name: &str,
        status: Option<AgentStatus>,
        since: Option<i64>,
    ) -> Session {
        Session {
            agent_status: status,
            agent_status_since: since,
            ..session(name)
        }
    }

    #[test]
    fn agent_sort_puts_attention_before_waiting_before_working_before_none() {
        let mut sessions = vec![
            session_with_agent_status("idle", None, None),
            session_with_agent_status("working", Some(AgentStatus::Working), Some(1)),
            session_with_agent_status("attention", Some(AgentStatus::Attention), Some(1)),
            session_with_agent_status("waiting", Some(AgentStatus::Waiting), Some(1)),
        ];

        sort_sessions_by_agent_status(&mut sessions);

        let names: Vec<&str> = sessions.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["attention", "waiting", "working", "idle"]);
    }

    #[test]
    fn urgent_agent_session_takes_cursor_from_current_session() {
        let mut sessions = vec![
            session_with_agent_status("current", None, None),
            session_with_agent_status("blocked", Some(AgentStatus::Attention), Some(1)),
        ];
        sort_sessions_by_agent_status(&mut sessions);
        let mut app = App::new(sessions, Some("current".to_string()));
        assert_eq!(app.selected_session().unwrap().name, "current");

        app.select_most_urgent_agent_session();

        assert_eq!(app.selected_session().unwrap().name, "blocked");
    }

    #[test]
    fn working_agent_does_not_take_cursor_from_current_session() {
        let mut sessions = vec![
            session_with_agent_status("current", None, None),
            session_with_agent_status("busy", Some(AgentStatus::Working), Some(1)),
        ];
        sort_sessions_by_agent_status(&mut sessions);
        let mut app = App::new(sessions, Some("current".to_string()));

        app.select_most_urgent_agent_session();

        assert_eq!(app.selected_session().unwrap().name, "current");
    }

    #[test]
    fn agent_sort_puts_current_session_last_within_its_rank() {
        let mut sessions = vec![
            session_with_agent_status("current", Some(AgentStatus::Waiting), Some(1)),
            session_with_agent_status("older", Some(AgentStatus::Waiting), Some(50)),
            session_with_agent_status("newer", Some(AgentStatus::Waiting), Some(100)),
            session_with_agent_status("busy", Some(AgentStatus::Working), Some(1)),
        ];

        sort_sessions_by_agent_status_with_current(&mut sessions, Some("current"));

        let names: Vec<&str> = sessions.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["older", "newer", "current", "busy"]);
    }

    #[test]
    fn agent_sort_keeps_untracked_current_session_in_tmux_order() {
        let mut sessions = vec![
            session_with_agent_status("current", None, None),
            session_with_agent_status("other", None, None),
        ];

        sort_sessions_by_agent_status_with_current(&mut sessions, Some("current"));

        let names: Vec<&str> = sessions.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["current", "other"]);
    }

    fn triage_sessions() -> Vec<Session> {
        vec![
            session_with_agent_status("idle", None, None),
            session_with_agent_status("c", Some(AgentStatus::Waiting), Some(300)),
            session_with_agent_status("busy", Some(AgentStatus::Working), Some(1)),
            session_with_agent_status("a", Some(AgentStatus::Waiting), Some(100)),
            session_with_agent_status("b", Some(AgentStatus::Waiting), Some(200)),
        ]
    }

    fn next_name<'a>(sessions: &'a [Session], current: &str) -> Option<&'a str> {
        next_urgent_session(sessions, Some(&format!("${current}")))
            .map(|session| session.name.as_str())
    }

    #[test]
    fn next_cycles_through_every_urgent_session_and_wraps() {
        let sessions = triage_sessions();

        assert_eq!(next_name(&sessions, "a"), Some("b"));
        assert_eq!(next_name(&sessions, "b"), Some("c"));
        assert_eq!(next_name(&sessions, "c"), Some("a"));
    }

    #[test]
    fn next_from_a_non_urgent_session_goes_to_the_most_urgent() {
        let mut sessions = triage_sessions();
        sessions.push(session_with_agent_status(
            "blocked",
            Some(AgentStatus::Attention),
            Some(999),
        ));

        assert_eq!(next_name(&sessions, "idle"), Some("blocked"));
        assert_eq!(next_name(&sessions, "busy"), Some("blocked"));
    }

    #[test]
    fn next_is_none_when_nothing_else_needs_you() {
        let sessions = vec![
            session_with_agent_status("only", Some(AgentStatus::Waiting), Some(1)),
            session_with_agent_status("busy", Some(AgentStatus::Working), Some(1)),
        ];

        assert_eq!(next_name(&sessions, "only"), None);
        assert_eq!(next_name(&sessions, "busy"), Some("only"));
        assert_eq!(next_name(&[session("idle")], "idle"), None);
    }

    #[test]
    fn agent_sort_breaks_ties_by_oldest_timestamp_first() {
        let mut sessions = vec![
            session_with_agent_status("just-now", Some(AgentStatus::Waiting), Some(500)),
            session_with_agent_status("neglected", Some(AgentStatus::Waiting), Some(100)),
        ];

        sort_sessions_by_agent_status(&mut sessions);

        let names: Vec<&str> = sessions.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["neglected", "just-now"]);
    }

    #[test]
    fn agent_sort_is_stable_for_sessions_with_no_agent() {
        let mut sessions = vec![
            session_with_agent_status("zeta", None, None),
            session_with_agent_status("alpha", None, None),
        ];

        sort_sessions_by_agent_status(&mut sessions);

        let names: Vec<&str> = sessions.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["zeta", "alpha"]);
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
    fn preserves_preview_for_matching_session_after_refresh() {
        let mut app = App::new(
            vec![session("dev"), session("logs")],
            Some("dev".to_string()),
        );
        app.sessions[0].preview = vec!["snapshot".to_string()];
        app.sessions[0].preview_error = None;

        let mut refreshed_dev = session("dev");
        refreshed_dev.preview = Vec::new();
        refreshed_dev.preview_error = Some("Current session preview disabled".to_string());

        let mut refreshed_logs = session("logs");
        refreshed_logs.preview = vec!["live".to_string()];

        app.replace_sessions_preserving_preview_for(
            vec![refreshed_dev, refreshed_logs],
            Some("$dev"),
        );

        assert_eq!(app.sessions[0].preview, vec!["snapshot".to_string()]);
        assert_eq!(app.sessions[0].preview_error, None);
        assert_eq!(app.sessions[1].preview, vec!["live".to_string()]);
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

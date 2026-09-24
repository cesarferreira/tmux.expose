use std::collections::HashMap;
use std::process::Command;

use anyhow::{Context, Result, anyhow};

use crate::model::{AgentPaneCounts, AgentStatus, Session};

const FIELD_SEPARATOR: char = ':';
const LEGACY_FIELD_SEPARATOR: char = '\u{1f}';
const SESSION_FORMAT: &str = "#{session_id}:#{session_name}:#{session_attached}:#{session_windows}:#{session_created}:#{session_activity}";
const PANE_AGENT_FORMAT: &str = "#{session_id}:#{@agent_status}:#{@agent_status_since}";

pub fn list_sessions() -> Result<Vec<Session>> {
    list_sessions_skipping_preview_for(None)
}

pub fn list_sessions_skipping_preview_for(
    current_session_id: Option<&str>,
) -> Result<Vec<Session>> {
    let mut sessions = list_sessions_without_previews()?;

    for session in &mut sessions {
        session.current_window = current_window_name(&session.id).unwrap_or(None);
        if !should_capture_preview(&session.id, current_session_id) {
            session.preview.clear();
            session.preview_error = Some("Current session preview disabled".to_string());
            continue;
        }

        match capture_session_preview(&session.id, 200) {
            Ok(preview) => {
                session.preview = preview;
                session.preview_error = None;
            }
            Err(error) => {
                session.preview.clear();
                session.preview_error = Some(error.to_string());
            }
        }
    }

    Ok(sessions)
}

/// Sessions with agent status but no window names or pane captures — the
/// cheap listing for headless commands like `tmux-expose next`, which run on
/// a keypress and never draw a card.
pub fn list_sessions_without_previews() -> Result<Vec<Session>> {
    let output = Command::new("tmux")
        .args(["list-sessions", "-F", SESSION_FORMAT])
        .output()
        .context("failed to run tmux list-sessions")?;

    if !output.status.success() {
        return Err(tmux_error("tmux list-sessions failed", &output.stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut sessions = parse_sessions_output(&stdout)?;

    let agent_rows = list_pane_agent_rows();
    apply_agent_status(&mut sessions, &agent_rows);

    Ok(sessions)
}

pub fn current_session_name() -> Result<Option<String>> {
    let output = Command::new("tmux")
        .args(["display-message", "-p", "#S"])
        .output()
        .context("failed to run tmux display-message")?;

    if !output.status.success() {
        return Ok(None);
    }

    let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok((!name.is_empty()).then_some(name))
}

pub fn current_session_id() -> Result<Option<String>> {
    let output = Command::new("tmux")
        .args(["display-message", "-p", "#{session_id}"])
        .output()
        .context("failed to run tmux display-message")?;

    if !output.status.success() {
        return Ok(None);
    }

    let id = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok((!id.is_empty()).then_some(id))
}

pub fn current_window_name(session_target: &str) -> Result<Option<String>> {
    let target = format!("{}:", session_target);
    let output = Command::new("tmux")
        .args(["display-message", "-p", "-t", &target, "#W"])
        .output()
        .with_context(|| format!("failed to query active window for session '{session_target}'"))?;

    if !output.status.success() {
        return Ok(None);
    }

    let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok((!name.is_empty()).then_some(name))
}

pub fn capture_session_preview(session_target: &str, max_lines: usize) -> Result<Vec<String>> {
    let target = format!("{}:", session_target);
    let output = Command::new("tmux")
        .args(["capture-pane", "-e", "-p", "-t", &target])
        .output()
        .with_context(|| format!("failed to capture pane for session '{session_target}'"))?;

    if !output.status.success() {
        return Err(tmux_error("tmux capture-pane failed", &output.stderr));
    }

    Ok(trim_preview(
        String::from_utf8_lossy(&output.stdout).as_ref(),
        max_lines,
    ))
}

pub fn switch_client(session_target: &str) -> Result<()> {
    switch_client_for(None, session_target)
}

/// Like `switch_client`, but for a specific client. Commands run from a key
/// binding via `run-shell` aren't attached to any client, so they have to
/// name the one that pressed the key or tmux guesses.
pub fn switch_client_for(client: Option<&str>, session_target: &str) -> Result<()> {
    let mut args = vec!["switch-client"];
    if let Some(client) = client {
        args.extend(["-c", client]);
    }
    args.extend(["-t", session_target]);

    let status = Command::new("tmux")
        .args(&args)
        .status()
        .with_context(|| format!("failed to switch to tmux session '{session_target}'"))?;

    if !status.success() {
        return Err(anyhow!("tmux switch-client failed for '{session_target}'"));
    }

    Ok(())
}

pub fn display_message(client: Option<&str>, message: &str) -> Result<()> {
    let mut args = vec!["display-message"];
    if let Some(client) = client {
        args.extend(["-c", client]);
    }
    args.push(message);

    let status = Command::new("tmux")
        .args(&args)
        .status()
        .context("failed to run tmux display-message")?;

    if !status.success() {
        return Err(anyhow!("tmux display-message failed"));
    }

    Ok(())
}

pub fn parse_sessions(output: &str) -> Vec<Session> {
    output
        .lines()
        .filter_map(|line| {
            let separator = if line.contains(FIELD_SEPARATOR) {
                FIELD_SEPARATOR
            } else {
                LEGACY_FIELD_SEPARATOR
            };
            let mut parts = line.splitn(6, separator);
            let id = parts.next()?.to_string();
            let name = parts.next()?.to_string();
            if name.is_empty() {
                return None;
            }

            let attached = parts.next().is_some_and(|value| value != "0");
            let window_count = parts
                .next()
                .and_then(|value| value.parse::<u32>().ok())
                .unwrap_or(0);
            let _created = parts.next();
            let last_activity = parts
                .next()
                .filter(|value| !value.is_empty())
                .map(ToString::to_string);

            Some(Session {
                id,
                name,
                attached,
                window_count,
                current_window: None,
                last_activity,
                preview: Vec::new(),
                preview_error: None,
                agent_status: None,
                agent_status_since: None,
                agent_pane_counts: AgentPaneCounts::default(),
            })
        })
        .collect()
}

/// Fetches `@agent_status`/`@agent_status_since` for every pane across every
/// session in one call. Best-effort: any failure (no panes, tmux hiccup)
/// just yields no rows, so a session simply shows no agent badge rather than
/// breaking the whole refresh.
fn list_pane_agent_rows() -> Vec<(String, Option<AgentStatus>, Option<i64>)> {
    let Ok(output) = Command::new("tmux")
        .args(["list-panes", "-a", "-F", PANE_AGENT_FORMAT])
        .output()
    else {
        return Vec::new();
    };

    if !output.status.success() {
        return Vec::new();
    }

    parse_pane_agent_rows(&String::from_utf8_lossy(&output.stdout))
}

fn parse_pane_agent_rows(output: &str) -> Vec<(String, Option<AgentStatus>, Option<i64>)> {
    output
        .lines()
        .filter_map(|line| {
            let mut parts = line.splitn(3, FIELD_SEPARATOR);
            let session_id = parts.next()?.to_string();
            if session_id.is_empty() {
                return None;
            }

            let status = parts.next().and_then(AgentStatus::parse);
            let since = parts.next().and_then(|value| value.parse::<i64>().ok());
            Some((session_id, status, since))
        })
        .collect()
}

/// Folds a session's panes down to one badge: the worst status present, and
/// how long the oldest pane at that status has held it.
#[derive(Default)]
struct AgentAggregate {
    counts: AgentPaneCounts,
    attention_since: Option<i64>,
    waiting_since: Option<i64>,
    working_since: Option<i64>,
}

impl AgentAggregate {
    fn record(&mut self, status: AgentStatus, since: Option<i64>) {
        let (count, since_slot) = match status {
            AgentStatus::Attention => (&mut self.counts.attention, &mut self.attention_since),
            AgentStatus::Waiting => (&mut self.counts.waiting, &mut self.waiting_since),
            AgentStatus::Working => (&mut self.counts.working, &mut self.working_since),
        };
        *count += 1;
        merge_oldest(since_slot, since);
    }

    fn worst(&self) -> Option<(AgentStatus, Option<i64>)> {
        if self.counts.attention > 0 {
            Some((AgentStatus::Attention, self.attention_since))
        } else if self.counts.waiting > 0 {
            Some((AgentStatus::Waiting, self.waiting_since))
        } else if self.counts.working > 0 {
            Some((AgentStatus::Working, self.working_since))
        } else {
            None
        }
    }
}

fn merge_oldest(slot: &mut Option<i64>, candidate: Option<i64>) {
    match (*slot, candidate) {
        (None, candidate) => *slot = candidate,
        (Some(current), Some(candidate)) if candidate < current => *slot = Some(candidate),
        _ => {}
    }
}

fn apply_agent_status(
    sessions: &mut [Session],
    rows: &[(String, Option<AgentStatus>, Option<i64>)],
) {
    let mut by_session: HashMap<&str, AgentAggregate> = HashMap::new();
    for (session_id, status, since) in rows {
        if let Some(status) = status {
            by_session
                .entry(session_id.as_str())
                .or_default()
                .record(*status, *since);
        }
    }

    for session in sessions.iter_mut() {
        let Some(aggregate) = by_session.get(session.id.as_str()) else {
            continue;
        };
        session.agent_pane_counts = aggregate.counts;
        (session.agent_status, session.agent_status_since) = match aggregate.worst() {
            Some((status, since)) => (Some(status), since),
            None => (None, None),
        };
    }
}

fn parse_sessions_output(output: &str) -> Result<Vec<Session>> {
    let sessions = parse_sessions(output);
    if sessions.is_empty() && !output.trim().is_empty() {
        return Err(anyhow!(
            "tmux list-sessions returned output in an unexpected format"
        ));
    }

    Ok(sessions)
}

pub fn trim_preview(output: &str, max_lines: usize) -> Vec<String> {
    let mut lines: Vec<String> = output
        .lines()
        .map(|line| line.trim_end().to_string())
        .collect();

    while lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }

    let start = lines.len().saturating_sub(max_lines);
    lines.into_iter().skip(start).collect()
}

fn tmux_error(message: &str, stderr: &[u8]) -> anyhow::Error {
    let stderr = String::from_utf8_lossy(stderr).trim().to_string();
    if stderr.is_empty() {
        anyhow!(message.to_string())
    } else {
        anyhow!("{message}: {stderr}")
    }
}

fn should_capture_preview(session_id: &str, current_session_id: Option<&str>) -> bool {
    current_session_id != Some(session_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_session_lines_from_tmux_format() {
        let sessions = parse_sessions(
            "$1\u{1f}dev\u{1f}1\u{1f}4\u{1f}1710000000\u{1f}1710000300\n$2\u{1f}logs\u{1f}0\u{1f}2\u{1f}1710000100\u{1f}1710000400\n",
        );

        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0].id, "$1");
        assert_eq!(sessions[0].name, "dev");
        assert!(sessions[0].attached);
        assert_eq!(sessions[0].window_count, 4);
        assert_eq!(sessions[0].last_activity.as_deref(), Some("1710000300"));
        assert_eq!(sessions[1].name, "logs");
        assert!(!sessions[1].attached);
    }

    #[test]
    fn parses_session_names_containing_pipe_characters() {
        let sessions =
            parse_sessions("$3\u{1f}dev|api\u{1f}0\u{1f}1\u{1f}1710000000\u{1f}1710000300\n");

        assert_eq!(sessions[0].id, "$3");
        assert_eq!(sessions[0].name, "dev|api");
    }

    #[test]
    fn parses_colon_delimited_session_lines() {
        let sessions = parse_sessions("$3:dev|api:0:1:1710000000:1710000300\n");

        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, "$3");
        assert_eq!(sessions[0].name, "dev|api");
        assert!(!sessions[0].attached);
        assert_eq!(sessions[0].window_count, 1);
        assert_eq!(sessions[0].last_activity.as_deref(), Some("1710000300"));
    }

    #[test]
    fn rejects_non_empty_unparseable_session_output() {
        let error = parse_sessions_output("$3dev011710000000171000300\n")
            .unwrap_err()
            .to_string();

        assert!(error.contains("unexpected format"));
    }

    #[test]
    fn trims_preview_to_last_non_empty_visible_lines() {
        let preview = trim_preview("first\nsecond\nthird\n\n", 2);

        assert_eq!(preview, vec!["second".to_string(), "third".to_string()]);
    }

    #[test]
    fn preserves_ansi_escape_sequences_from_preview() {
        let preview = trim_preview("\u{1b}[31mred\u{1b}[0m plain", 5);

        assert_eq!(preview, vec!["\u{1b}[31mred\u{1b}[0m plain".to_string()]);
    }

    #[test]
    fn skips_preview_capture_for_current_session_only_when_requested() {
        assert!(!should_capture_preview("$1", Some("$1")));
        assert!(should_capture_preview("$2", Some("$1")));
        assert!(should_capture_preview("$1", None));
    }

    #[test]
    fn parses_pane_agent_rows() {
        let rows = parse_pane_agent_rows("$1:waiting:1700000000\n$2:working:1700000100\n");

        assert_eq!(
            rows,
            vec![
                (
                    "$1".to_string(),
                    Some(AgentStatus::Waiting),
                    Some(1700000000)
                ),
                (
                    "$2".to_string(),
                    Some(AgentStatus::Working),
                    Some(1700000100)
                ),
            ]
        );
    }

    #[test]
    fn parses_pane_with_no_agent_status_as_none() {
        // Unset tmux pane options render as empty fields, not missing ones.
        let rows = parse_pane_agent_rows("$1::\n");

        assert_eq!(rows, vec![("$1".to_string(), None, None)]);
    }

    #[test]
    fn ignores_unrecognized_status_values() {
        // Status and timestamp parse independently — an unrecognized status
        // still leaves a well-formed timestamp field intact.
        let rows = parse_pane_agent_rows("$1:not-a-real-status:123\n");

        assert_eq!(rows, vec![("$1".to_string(), None, Some(123))]);
    }

    #[test]
    fn session_badge_is_the_worst_pane_status_present() {
        let mut sessions = vec![test_session("$1")];
        let rows = vec![
            ("$1".to_string(), Some(AgentStatus::Working), Some(100)),
            ("$1".to_string(), Some(AgentStatus::Waiting), Some(50)),
            ("$1".to_string(), Some(AgentStatus::Attention), Some(200)),
        ];

        apply_agent_status(&mut sessions, &rows);

        assert_eq!(sessions[0].agent_status, Some(AgentStatus::Attention));
        assert_eq!(sessions[0].agent_status_since, Some(200));
        assert_eq!(
            sessions[0].agent_pane_counts,
            AgentPaneCounts {
                working: 1,
                waiting: 1,
                attention: 1,
            }
        );
    }

    #[test]
    fn badge_timestamp_is_the_oldest_pane_at_the_worst_rank() {
        let mut sessions = vec![test_session("$1")];
        let rows = vec![
            ("$1".to_string(), Some(AgentStatus::Waiting), Some(500)),
            ("$1".to_string(), Some(AgentStatus::Waiting), Some(100)),
        ];

        apply_agent_status(&mut sessions, &rows);

        assert_eq!(sessions[0].agent_status, Some(AgentStatus::Waiting));
        assert_eq!(sessions[0].agent_status_since, Some(100));
    }

    #[test]
    fn sessions_with_no_matching_pane_rows_stay_agent_free() {
        let mut sessions = vec![test_session("$1")];

        apply_agent_status(&mut sessions, &[]);

        assert_eq!(sessions[0].agent_status, None);
        assert_eq!(sessions[0].agent_status_since, None);
    }

    fn test_session(id: &str) -> Session {
        Session {
            id: id.to_string(),
            name: id.to_string(),
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
}

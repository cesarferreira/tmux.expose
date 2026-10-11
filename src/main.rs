use std::{
    env, io,
    process::Command,
    str::FromStr,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
use clap::Parser;
use crossterm::{
    cursor::{Hide, Show},
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::style::Color;
use ratatui::{Terminal, backend::CrosstermBackend, layout::Rect};
use tmux_expose::{
    input,
    model::{self, AgentStatus, App},
    tmux, ui,
};

#[derive(Debug, Parser)]
#[command(version, about = "Mission Control-style tmux session switcher")]
struct Cli {
    #[arg(long, default_value_t = 500, value_name = "MS", value_parser = clap::value_parser!(u64).range(1..))]
    refresh_interval: u64,

    #[arg(long, value_name = "COLS", value_parser = clap::value_parser!(u16).range(1..))]
    thumbnail_width: Option<u16>,

    #[arg(long, value_name = "N", value_parser = clap::value_parser!(u16).range(1..))]
    columns: Option<u16>,

    #[arg(long, value_name = "COLOR", value_parser = parse_color)]
    selected_color: Option<Color>,

    #[arg(long, value_name = "COLOR", value_parser = parse_color)]
    attached_color: Option<Color>,

    #[arg(long, value_name = "COLOR", value_parser = parse_color)]
    inactive_color: Option<Color>,

    #[arg(long, value_name = "COLOR", value_parser = parse_color)]
    attention_color: Option<Color>,

    #[arg(long, value_name = "COLOR", value_parser = parse_color)]
    waiting_color: Option<Color>,

    #[arg(long, value_name = "COLOR", value_parser = parse_color)]
    working_color: Option<Color>,

    /// Use modal vim navigation: hjkl to move, `/` to search, q/Esc to quit.
    #[arg(long)]
    vim: bool,

    /// Keep tmux's own session order instead of sorting sessions with a
    /// waiting agent to the top.
    #[arg(long)]
    no_agent_sort: bool,
}

fn parse_color(value: &str) -> Result<Color, String> {
    if let Ok(color) = Color::from_str(value) {
        return Ok(color);
    }

    // Accept tmux-style indexed colors such as `colour208` / `color208`,
    // which ratatui's parser does not recognize on its own.
    if let Some(index) = value
        .strip_prefix("colour")
        .or_else(|| value.strip_prefix("color"))
        .and_then(|digits| digits.parse::<u8>().ok())
    {
        return Ok(Color::Indexed(index));
    }

    Err(format!(
        "invalid color {value:?}; expected a name (e.g. `yellow`), \
         an index (`208` or `colour208`), or a hex value (`#rrggbb`)"
    ))
}

struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> Result<Self> {
        enable_raw_mode().context("failed to enable raw mode")?;
        let guard = Self;
        execute!(io::stdout(), EnterAlternateScreen, Hide, EnableMouseCapture)
            .context("failed to enter alternate screen")?;
        Ok(guard)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(
            io::stdout(),
            Show,
            LeaveAlternateScreen,
            DisableMouseCapture
        );
    }
}

/// One-shot side-effecting command meant to be wired straight into an
/// agent's own hook config (e.g. Claude Code's `settings.json`) as
/// `tmux-expose agent-status <working|waiting|attention|clear>`. Deliberately
/// not a clap subcommand: it never launches the TUI, so hook configs only
/// need to know one binary name, and it can't collide with the picker's own
/// flags.
///
/// Records status on the calling pane (via `$TMUX_PANE`, which tmux injects
/// into every pane's shell — and which a hook subprocess inherits, since
/// it's a child of that same shell) so tmux-expose can read it back later.
fn run_agent_status(status: &str) -> Result<()> {
    validate_agent_status_word(status)?;

    let Ok(pane) = env::var("TMUX_PANE") else {
        return Ok(()); // Not inside tmux — nothing to record.
    };
    if pane.is_empty() {
        return Ok(());
    }

    if status == "clear" {
        clear_pane_option(&pane, "@agent_status");
        clear_pane_option(&pane, "@agent_status_since");
    } else {
        set_pane_option(&pane, "@agent_status", status);
        let since = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|elapsed| elapsed.as_secs().to_string())
            .unwrap_or_default();
        set_pane_option(&pane, "@agent_status_since", &since);
    }

    Ok(())
}

/// Headless triage: jumps straight to the next session whose agent is
/// blocked on or waiting for you (see `model::next_urgent_session`), without
/// opening the picker. Like `agent-status`, deliberately not a clap
/// subcommand, so it never touches the picker's flags.
///
/// Usage: `tmux-expose next [SESSION_ID [CLIENT]]`. The plugin's key binding
/// passes `#{session_id}` and `#{client_name}`, because `run-shell` isn't
/// attached to a client and tmux would otherwise guess which one you meant.
/// Run from a shell inside a pane, both can be omitted.
fn run_next(session_id: Option<String>, client: Option<String>) -> Result<()> {
    let session_id = match session_id.filter(|id| !id.is_empty()) {
        Some(id) => Some(id),
        None => tmux::current_session_id().unwrap_or(None),
    };
    let client = client.filter(|client| !client.is_empty());

    let sessions = tmux::list_sessions_without_previews()?;
    match model::next_urgent_session(&sessions, session_id.as_deref()) {
        Some(session) => tmux::switch_client_for(client.as_deref(), &session.id),
        None => tmux::display_message(
            client.as_deref(),
            "tmux-expose: no other agents waiting on you",
        ),
    }
}

fn validate_agent_status_word(status: &str) -> Result<()> {
    if status == "clear" || AgentStatus::parse(status).is_some() {
        return Ok(());
    }
    bail!("unknown agent status {status:?}; expected working, waiting, attention, or clear");
}

// A hook should never fail loudly just because tmux hiccuped — swallow the
// result rather than propagating it.
fn set_pane_option(pane: &str, option: &str, value: &str) {
    let _ = Command::new("tmux")
        .args(["set-option", "-p", "-t", pane, option, value])
        .status();
}

fn clear_pane_option(pane: &str, option: &str) {
    let _ = Command::new("tmux")
        .args(["set-option", "-p", "-t", pane, "-u", option])
        .status();
}

fn main() -> Result<()> {
    let mut raw_args = env::args();
    raw_args.next(); // program name
    match raw_args.next().as_deref() {
        Some("agent-status") => return run_agent_status(&raw_args.next().unwrap_or_default()),
        Some("next") => return run_next(raw_args.next(), raw_args.next()),
        _ => {}
    }

    let cli = Cli::parse();

    let current_session_name = tmux::current_session_name().unwrap_or(None);
    let current_session_id = tmux::current_session_id().unwrap_or(None);
    let agent_sort = !cli.no_agent_sort;
    let mut app = match tmux::list_sessions() {
        Ok(mut sessions) => {
            if agent_sort {
                model::sort_sessions_by_agent_status_with_current(
                    &mut sessions,
                    current_session_name.as_deref(),
                );
            }
            let mut app = App::new(sessions, current_session_name);
            if agent_sort {
                app.select_most_urgent_agent_session();
            }
            app
        }
        Err(error) => {
            let mut app = App::new(Vec::new(), current_session_name);
            app.error = Some(format!("{error}\n\nPress q or Esc to quit."));
            app
        }
    };
    app.vim_keys = cli.vim;

    let mut colors = ui::CardColors::default();
    if let Some(color) = cli.selected_color {
        colors.selected = color;
    }
    if let Some(color) = cli.attached_color {
        colors.attached = color;
    }
    if let Some(color) = cli.inactive_color {
        colors.inactive = color;
    }
    if let Some(color) = cli.attention_color {
        colors.attention = color;
    }
    if let Some(color) = cli.waiting_color {
        colors.waiting = color;
    }
    if let Some(color) = cli.working_color {
        colors.working = color;
    }

    let _guard = TerminalGuard::enter()?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend).context("failed to create terminal")?;

    let refresh_interval = Duration::from_millis(cli.refresh_interval);
    let toggle_key = env::var("TMUX_EXPOSE_TOGGLE_KEY")
        .ok()
        .and_then(|key| input::ToggleKey::from_tmux_key(&key));
    let mut last_refresh = Instant::now();

    loop {
        let forced_columns = cli.columns.map(usize::from);
        terminal
            .draw(|frame| ui::render(frame, &app, colors, cli.thumbnail_width, forced_columns))?;

        if app.should_quit {
            break;
        }

        if app.should_switch {
            if let Some(session) = app.selected_session() {
                let selected_name = session.name.clone();
                let selected_target = session.id.clone();
                if app.current_session_name.as_deref() == Some(selected_name.as_str()) {
                    break;
                }

                match tmux::switch_client(&selected_target) {
                    Ok(()) => break,
                    Err(error) => {
                        app.error = Some(format!("{error}\n\nPress q or Esc to quit."));
                        app.should_switch = false;
                    }
                }
            } else {
                app.should_switch = false;
            }
        }

        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(key)
                    if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) =>
                {
                    let columns = current_columns(
                        &terminal,
                        app.visible_session_count(),
                        cli.thumbnail_width,
                        forced_columns,
                    )?;
                    input::handle_key_with_toggle(&mut app, key, columns, toggle_key);
                }
                Event::Mouse(mouse) => {
                    let grid_area = current_grid_area(&terminal)?;
                    input::handle_mouse(
                        &mut app,
                        mouse,
                        grid_area,
                        cli.thumbnail_width,
                        forced_columns,
                    );
                }
                Event::Resize(_, _) => {}
                _ => {}
            }
        }

        if last_refresh.elapsed() >= refresh_interval {
            match tmux::list_sessions_skipping_preview_for(current_session_id.as_deref()) {
                Ok(mut sessions) => {
                    if agent_sort {
                        model::sort_sessions_by_agent_status_with_current(
                            &mut sessions,
                            app.current_session_name.as_deref(),
                        );
                    }
                    app.replace_sessions_preserving_preview_for(
                        sessions,
                        current_session_id.as_deref(),
                    );
                    app.error = None;
                }
                Err(error) => {
                    app.error = Some(format!("{error}\n\nPress q or Esc to quit."));
                }
            }
            last_refresh = Instant::now();
        }
    }

    Ok(())
}

fn current_columns(
    terminal: &Terminal<CrosstermBackend<io::Stdout>>,
    session_count: usize,
    min_card_width: Option<u16>,
    forced_columns: Option<usize>,
) -> Result<usize> {
    let area = current_grid_area(terminal)?;
    let grid = ui::calculate_grid(area, session_count, min_card_width, forced_columns);
    Ok(grid.columns)
}

fn current_grid_area(terminal: &Terminal<CrosstermBackend<io::Stdout>>) -> Result<Rect> {
    let area = terminal.size().context("failed to read terminal size")?;
    Ok(Rect::new(0, 0, area.width, area.height.saturating_sub(1)))
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;

    #[test]
    fn thumbnail_width_defaults_to_fit_screen_mode() {
        let cli = Cli::parse_from(["tmux-expose"]);

        assert_eq!(cli.thumbnail_width, None);
    }

    #[test]
    fn parses_thumbnail_width_option() {
        let cli = Cli::parse_from(["tmux-expose", "--thumbnail-width", "48"]);

        assert_eq!(cli.thumbnail_width, Some(48));
    }

    #[test]
    fn parses_forced_columns_option() {
        let cli = Cli::parse_from(["tmux-expose", "--columns", "2"]);

        assert_eq!(cli.columns, Some(2));
    }

    #[test]
    fn color_options_default_to_none() {
        let cli = Cli::parse_from(["tmux-expose"]);

        assert_eq!(cli.selected_color, None);
        assert_eq!(cli.attached_color, None);
        assert_eq!(cli.inactive_color, None);
    }

    #[test]
    fn parses_named_color() {
        let cli = Cli::parse_from(["tmux-expose", "--selected-color", "cyan"]);

        assert_eq!(cli.selected_color, Some(Color::Cyan));
    }

    #[test]
    fn parses_indexed_color() {
        let cli = Cli::parse_from(["tmux-expose", "--attached-color", "208"]);

        assert_eq!(cli.attached_color, Some(Color::Indexed(208)));
    }

    #[test]
    fn parses_all_color_options() {
        let cli = Cli::parse_from([
            "tmux-expose",
            "--selected-color",
            "magenta",
            "--attached-color",
            "green",
            "--inactive-color",
            "white",
        ]);

        assert_eq!(cli.selected_color, Some(Color::Magenta));
        assert_eq!(cli.attached_color, Some(Color::Green));
        assert_eq!(cli.inactive_color, Some(Color::White));
    }

    #[test]
    fn parses_hex_color() {
        let cli = Cli::parse_from(["tmux-expose", "--selected-color", "#ff8700"]);

        assert_eq!(cli.selected_color, Some(Color::Rgb(255, 135, 0)));
    }

    #[test]
    fn parses_tmux_style_indexed_color() {
        // tmux spells indexed colors as `colour208`; ratatui only accepts `208`.
        assert_eq!(parse_color("colour208"), Ok(Color::Indexed(208)));
        assert_eq!(parse_color("color208"), Ok(Color::Indexed(208)));
        assert_eq!(parse_color("208"), Ok(Color::Indexed(208)));
    }

    #[test]
    fn rejects_invalid_color() {
        let result = Cli::try_parse_from(["tmux-expose", "--selected-color", "not-a-color"]);

        assert!(result.is_err());
        // `colour` prefix with a non-numeric / out-of-range suffix is still invalid.
        assert!(parse_color("colourize").is_err());
        assert!(parse_color("colour999").is_err());
    }

    #[test]
    fn vim_defaults_to_off() {
        let cli = Cli::parse_from(["tmux-expose"]);

        assert!(!cli.vim);
    }

    #[test]
    fn parses_vim_flag() {
        let cli = Cli::parse_from(["tmux-expose", "--vim"]);

        assert!(cli.vim);
    }

    #[test]
    fn agent_status_colors_default_to_none() {
        let cli = Cli::parse_from(["tmux-expose"]);

        assert_eq!(cli.attention_color, None);
        assert_eq!(cli.waiting_color, None);
        assert_eq!(cli.working_color, None);
    }

    #[test]
    fn parses_agent_status_colors() {
        let cli = Cli::parse_from([
            "tmux-expose",
            "--attention-color",
            "red",
            "--waiting-color",
            "colour208",
            "--working-color",
            "#8be9fd",
        ]);

        assert_eq!(cli.attention_color, Some(Color::Red));
        assert_eq!(cli.waiting_color, Some(Color::Indexed(208)));
        assert_eq!(cli.working_color, Some(Color::Rgb(139, 233, 253)));
    }

    #[test]
    fn agent_sort_is_on_by_default() {
        let cli = Cli::parse_from(["tmux-expose"]);

        assert!(!cli.no_agent_sort);
    }

    #[test]
    fn parses_no_agent_sort_flag() {
        let cli = Cli::parse_from(["tmux-expose", "--no-agent-sort"]);

        assert!(cli.no_agent_sort);
    }

    #[test]
    fn accepts_known_agent_status_words() {
        for word in ["working", "waiting", "attention", "clear"] {
            assert!(
                validate_agent_status_word(word).is_ok(),
                "{word} should be valid"
            );
        }
    }

    #[test]
    fn rejects_unknown_agent_status_words() {
        assert!(validate_agent_status_word("").is_err());
        assert!(validate_agent_status_word("done").is_err());
        assert!(validate_agent_status_word("Waiting").is_err());
    }
}

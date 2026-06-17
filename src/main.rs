use std::{
    env, io,
    str::FromStr,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use clap::Parser;
use crossterm::{
    cursor::{Hide, Show},
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::style::Color;
use ratatui::{Terminal, backend::CrosstermBackend, layout::Rect};
use tmux_expose::{input, model::App, tmux, ui};

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

    /// Use modal vim navigation: hjkl to move, `/` to search, q/Esc to quit.
    #[arg(long)]
    vim: bool,
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

fn main() -> Result<()> {
    let cli = Cli::parse();

    let current_session_name = tmux::current_session_name().unwrap_or(None);
    let current_session_id = tmux::current_session_id().unwrap_or(None);
    let mut app = match tmux::list_sessions() {
        Ok(sessions) => App::new(sessions, current_session_name),
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
                Ok(sessions) => {
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
}

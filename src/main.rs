use std::{
    io,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use clap::Parser;
use crossterm::{
    cursor::{Hide, Show},
    event::{self, Event, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
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
}

struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> Result<Self> {
        enable_raw_mode().context("failed to enable raw mode")?;
        let guard = Self;
        execute!(io::stdout(), EnterAlternateScreen, Hide)
            .context("failed to enter alternate screen")?;
        Ok(guard)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), Show, LeaveAlternateScreen);
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let _guard = TerminalGuard::enter()?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend).context("failed to create terminal")?;

    let current_session_name = tmux::current_session_name().unwrap_or(None);
    let current_session_id = tmux::current_session_id().unwrap_or(None);
    let mut app = match tmux::list_sessions_skipping_preview_for(current_session_id.as_deref()) {
        Ok(sessions) => App::new(sessions, current_session_name),
        Err(error) => {
            let mut app = App::new(Vec::new(), current_session_name);
            app.error = Some(format!("{error}\n\nPress q or Esc to quit."));
            app
        }
    };

    let refresh_interval = Duration::from_millis(cli.refresh_interval);
    let mut last_refresh = Instant::now();

    loop {
        let forced_columns = cli.columns.map(usize::from);
        terminal.draw(|frame| ui::render(frame, &app, cli.thumbnail_width, forced_columns))?;

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
                    input::handle_key(&mut app, key, columns);
                }
                Event::Resize(_, _) => {}
                _ => {}
            }
        }

        if last_refresh.elapsed() >= refresh_interval {
            match tmux::list_sessions_skipping_preview_for(current_session_id.as_deref()) {
                Ok(sessions) => {
                    app.replace_sessions(sessions);
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
    let area = terminal.size().context("failed to read terminal size")?;
    let body_height = area.height.saturating_sub(1);
    let grid = ui::calculate_grid(
        Rect::new(0, 0, area.width, body_height),
        session_count,
        min_card_width,
        forced_columns,
    );
    Ok(grid.columns)
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
}

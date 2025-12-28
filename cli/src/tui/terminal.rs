use std::io;

use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use crossterm::{execute, terminal};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

pub fn check_tui_support() -> Result<(), String> {
    if !atty::is(atty::Stream::Stdout) {
        return Err("stdout is not a terminal".to_string());
    }
    if !cfg!(windows) && std::env::var("TERM").is_err() {
        return Err("TERM environment variable not set".to_string());
    }
    let (width, height) = terminal::size().map_err(|e| format!("terminal size failed: {e}"))?;
    if width < 80 || height < 24 {
        return Err(format!(
            "terminal too small ({}x{}), need at least 80x24",
            width, height
        ));
    }
    Ok(())
}

pub fn setup_terminal() -> Result<Terminal<CrosstermBackend<io::Stdout>>, String> {
    enable_raw_mode().map_err(|e| e.to_string())?;
    let mut stdout = io::stdout();
    execute!(stdout, terminal::EnterAlternateScreen, EnableMouseCapture)
        .map_err(|e| e.to_string())?;
    let backend = CrosstermBackend::new(stdout);
    Terminal::new(backend).map_err(|e| e.to_string())
}

pub fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) {
    let _ = disable_raw_mode();
    let _ = execute!(
        terminal.backend_mut(),
        terminal::LeaveAlternateScreen,
        DisableMouseCapture
    );
    let _ = terminal.show_cursor();
}

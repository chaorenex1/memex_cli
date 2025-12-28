mod app;
mod events;
mod ui;

use std::io;
use std::time::Duration;

use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use crossterm::{execute, terminal};
use memex_core::config::TuiConfig;
use memex_core::error::RunnerError;
use memex_core::runner::RunnerResult;
use memex_core::tui::TuiEvent;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::sync::mpsc;

pub use app::TuiApp;
use app::{InputMode, PromptAction};

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

#[allow(dead_code)]
pub async fn run_with_tui(
    run_id: String,
    cfg: &TuiConfig,
    event_rx: mpsc::UnboundedReceiver<TuiEvent>,
    run_task: tokio::task::JoinHandle<Result<RunnerResult, RunnerError>>,
) -> Result<RunnerResult, RunnerError> {
    let mut app = TuiApp::new(cfg.clone(), run_id);
    let mut terminal = setup_terminal().map_err(RunnerError::Spawn)?;
    let result = run_with_tui_on_terminal(&mut terminal, &mut app, event_rx, run_task).await;
    restore_terminal(&mut terminal);
    result
}

pub async fn run_with_tui_on_terminal(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut TuiApp,
    mut event_rx: mpsc::UnboundedReceiver<TuiEvent>,
    mut run_task: tokio::task::JoinHandle<Result<RunnerResult, RunnerError>>,
) -> Result<RunnerResult, RunnerError> {
    let (input_reader, mut input_rx) = events::InputReader::start();
    let mut tick =
        tokio::time::interval(Duration::from_millis(app.config.update_interval_ms.max(16)));

    let mut run_result: Option<Result<RunnerResult, RunnerError>> = None;
    let mut exit_requested = false;

    loop {
        tokio::select! {
            Some(event) = event_rx.recv() => {
                app.handle_event(event);
            }
            Some(key) = input_rx.recv() => {
                if app.handle_key(key) {
                    exit_requested = true;
                }
            }
            res = &mut run_task => {
                let res = match res {
                    Ok(inner) => inner,
                    Err(e) => Err(RunnerError::Spawn(format!("runner task failed: {e}"))),
                };
                if let Ok(ref result) = res {
                    if !app.is_done() {
                        app.status = app::RunStatus::Completed(result.exit_code);
                    }
                } else if let Err(ref err) = res {
                    app.status = app::RunStatus::Error(err.to_string());
                }
                run_result = Some(res);
            }
            _ = tick.tick() => {}
        }

        app.maybe_hide_splash();
        terminal
            .draw(|f| ui::draw(f, app))
            .map_err(|e| RunnerError::Spawn(e.to_string()))?;

        if app.is_done() || exit_requested {
            break;
        }
    }

    input_reader.stop();

    if let Some(result) = run_result {
        return result;
    }

    match run_task.await {
        Ok(result) => result,
        Err(e) => Err(RunnerError::Spawn(format!("runner task failed: {e}"))),
    }
}

pub async fn prompt_in_tui(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut TuiApp,
) -> Result<String, RunnerError> {
    app.set_prompt_mode();
    let (input_reader, mut input_rx) = events::InputReader::start();
    let mut tick =
        tokio::time::interval(Duration::from_millis(app.config.update_interval_ms.max(16)));

    loop {
        tokio::select! {
            Some(key) = input_rx.recv() => {
                match app.handle_prompt_key(key) {
                    PromptAction::Submit => break,
                    PromptAction::Cancel => {
                        input_reader.stop();
                        return Err(RunnerError::Spawn("input cancelled".to_string()));
                    }
                    PromptAction::Exit => {
                        input_reader.stop();
                        return Err(RunnerError::Spawn("input cancelled".to_string()));
                    }
                    PromptAction::None => {}
                }
            }
            _ = tick.tick() => {}
        }

        terminal
            .draw(|f| ui::draw(f, app))
            .map_err(|e| RunnerError::Spawn(e.to_string()))?;
    }

    input_reader.stop();
    app.input_mode = InputMode::Normal;
    Ok(app.input_buffer.clone())
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

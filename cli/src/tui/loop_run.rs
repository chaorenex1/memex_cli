use std::time::Duration;

use memex_core::config::TuiConfig;
use memex_core::error::RunnerError;
use memex_core::runner::RunnerResult;
use memex_core::tui::TuiEvent;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::sync::mpsc;

use super::events::{InputEvent, InputReader};
use super::terminal::{restore_terminal, setup_terminal};
use super::ui;
use super::TuiApp;

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
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    app: &mut TuiApp,
    mut event_rx: mpsc::UnboundedReceiver<TuiEvent>,
    mut run_task: tokio::task::JoinHandle<Result<RunnerResult, RunnerError>>,
) -> Result<RunnerResult, RunnerError> {
    tracing::debug!("TUI event loop starting");
    let (input_reader, mut input_rx) = InputReader::start();
    let mut tick =
        tokio::time::interval(Duration::from_millis(app.config.update_interval_ms.max(16)));

    let mut run_result: Option<Result<RunnerResult, RunnerError>> = None;
    let mut exit_requested = false;

    // Initial render to show the UI immediately
    app.maybe_hide_splash();
    terminal
        .draw(|f| ui::draw(f, app))
        .map_err(|e| RunnerError::Spawn(e.to_string()))?;

    loop {
        tokio::select! {
            Some(event) = event_rx.recv() => {
                app.handle_event(event);
            }
            Some(input_event) = input_rx.recv() => {
                match input_event {
                    InputEvent::Key(key) => {
                        if app.handle_key(key) {
                            exit_requested = true;
                        }
                    }
                    InputEvent::Mouse(_) => {
                        // Ignore mouse events in normal execution mode
                    }
                }
            }
            res = &mut run_task => {
                let res = match res {
                    Ok(inner) => inner,
                    Err(e) => Err(RunnerError::Spawn(format!("runner task failed: {e}"))),
                };
                if let Ok(ref result) = res {
                    tracing::debug!("run_task completed successfully, exit_code={}", result.exit_code);
                    if !app.is_done() {
                        app.status = super::app::RunStatus::Completed(result.exit_code);
                    }
                } else if let Err(ref err) = res {
                    tracing::debug!("run_task failed: {}", err);
                    app.status = super::app::RunStatus::Error(err.to_string());
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
            tracing::debug!("Exiting TUI loop: is_done={}, exit_requested={}", app.is_done(), exit_requested);
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

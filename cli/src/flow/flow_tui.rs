use std::sync::Arc;

use memex_core::config::{ControlConfig, TuiConfig};
use memex_core::error::RunnerError;
use memex_core::events_out::EventsOutTx;
use memex_core::memory::MemoryPlugin;
use memex_core::runner::{run_session, PolicyPlugin, RunnerResult, RunnerSession};
use memex_core::state::types::{RuntimePhase, StateEvent};
use memex_core::state::StateManager;
use memex_core::tui::TuiEvent;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::sync::mpsc;
use memex_plugins::factory;
use uuid::Uuid;

use crate::commands::cli::{Args, RunArgs};
use crate::flow::flow_qa::run_with_query;
use crate::tui::{restore_terminal, run_with_tui_on_terminal, setup_terminal, TuiApp};

// Unified error handling for TUI
fn handle_tui_error(tui_app: &mut TuiApp, error: &str, severity: &str) {
    let formatted = format!("[{}] {}", severity, error);
    tracing::error!("{}", formatted);
    tui_app.push_error_line(formatted);
    if severity == "ERROR" {
        tui_app.status = crate::tui::RunStatus::Error(error.to_string());
    }
}

pub struct TuiRuntime {
    pub terminal: Terminal<CrosstermBackend<std::io::Stdout>>,
    pub app: TuiApp,
}

impl TuiRuntime {
    pub fn new(cfg: &TuiConfig, run_id: String) -> Result<Self, RunnerError> {
        let terminal = setup_terminal().map_err(RunnerError::Spawn)?;
        let app = TuiApp::new(cfg.clone(), run_id);
        Ok(Self { terminal, app })
    }

    pub fn restore(&mut self) {
        restore_terminal(&mut self.terminal);
    }
}

pub async fn run_tui_flow(
    args: &Args,
    run_args: Option<&RunArgs>,
    cfg: &mut memex_core::config::AppConfig,
    state_manager: Option<Arc<StateManager>>,
    events_out_tx: Option<EventsOutTx>,
    run_id: String,
    recover_run_id: Option<String>,
    stream_enabled: bool,
    stream_format: &str,
    stream_silent: bool,
    policy: Option<Box<dyn PolicyPlugin>>,
    memory: Option<Box<dyn MemoryPlugin>>,
    _gatekeeper: Box<dyn memex_core::gatekeeper::GatekeeperPlugin>,
) -> Result<i32, RunnerError> {
    let mut tui = TuiRuntime::new(&cfg.tui, run_id.clone())?;
    
    tracing::debug!("TUI: Starting interactive loop");
    
    use crate::tui::events::{InputReader, InputEvent};
    use crate::tui::ui;
    use std::time::Duration;

    let mut last_exit_code = 0;

    // Main interactive loop - keep prompting until user explicitly exits
    'main_loop: loop {
        // Create input reader and tick for this query cycle
        let (input_reader, mut input_rx) = InputReader::start();
        let mut tick = tokio::time::interval(Duration::from_millis(tui.app.config.update_interval_ms.max(16)));
        
        // Reset app state for new query
        tui.app.reset_for_new_query();
        tui.app.set_prompt_mode();
        
        // Initial render
        tui.app.maybe_hide_splash();
        let input_area = tui.terminal.get_frame().area();
        if let Err(e) = tui.terminal.draw(|f| ui::draw(f, &mut tui.app)) {
            handle_tui_error(&mut tui.app, &format!("Initial render failed: {}", e), "WARN");
        }

        // Phase 1: Get user input
        tracing::debug!("TUI: Waiting for input");
        let user_input = loop {
            tokio::select! {
                Some(event) = input_rx.recv() => {
                    match event {
                        InputEvent::Key(key) => {
                            tracing::debug!("Prompt: Received key event: {:?}", key);
                            use crate::tui::PromptAction;
                            match tui.app.handle_prompt_key(key) {
                                PromptAction::Submit => {
                                    let input = tui.app.input_buffer.clone();
                                    if !input.is_empty() {
                                        break input;
                                    }
                                }
                                PromptAction::Clear => {
                                    tui.app.input_buffer.clear();
                                    tui.app.input_cursor = 0;
                                    tui.app.clear_selection();
                                }
                                PromptAction::Exit => {
                                    tracing::debug!("TUI: User requested exit from prompt");
                                    break 'main_loop;
                                }
                                PromptAction::None => {}
                            }
                        }
                        InputEvent::Mouse(mouse) => {
                            tui.app.handle_mouse(mouse, input_area);
                        }
                    }
                }
                _ = tick.tick() => {}
            }

            if let Err(e) = tui.terminal.draw(|f| ui::draw(f, &mut tui.app)) {
                handle_tui_error(&mut tui.app, &format!("Render error: {}", e), "WARN");
            }
        };

        tracing::debug!("TUI: Input received: {:?}", user_input);
        
        // Phase 2: Prepare for execution
        tui.app.input_buffer.clear();
        tui.app.input_cursor = 0;
        tui.app.input_mode = crate::tui::InputMode::Normal;
        tui.app.pending_qa = true;
        tui.app.qa_started_at = Some(std::time::Instant::now());

        // Phase 3: Execute query with continuing event loop
        let tui_ptr = &mut tui as *mut TuiRuntime;
        
        // Generate a new run_id for each query
        let query_run_id = Uuid::new_v4().to_string();
        tui.app.run_id = query_run_id.clone();
        
        // Rebuild plugins for each query
        let query_memory = factory::build_memory(&cfg).map_err(|e| {
            tracing::error!("Failed to build memory plugin: {}", e);
            RunnerError::Spawn(e.to_string())
        })?;
        let query_policy = factory::build_policy(&cfg);
        let query_gatekeeper = factory::build_gatekeeper(&cfg);
        
        let result = run_with_query(
            user_input,
            args,
            run_args,
            cfg,
            state_manager.clone(),
            events_out_tx.clone(),
            query_run_id,
            None, // No recovery for subsequent queries
            true,
            stream_enabled,
            stream_format,
            stream_silent,
            query_policy,
            query_memory,
            query_gatekeeper,
            Some(tui_ptr),
            |input| async move {
                run_tui_session_continuing(
                    unsafe { &mut *tui_ptr },
                    input,
                    &mut input_rx,
                    &mut tick,
                )
                .await
            },
        )
        .await;

        // Handle result and wait for user to review before next prompt
        match result {
            Ok(code) => {
                last_exit_code = code;
                tracing::debug!("TUI: Query completed with exit code: {}", code);
                tui.app.status = crate::tui::RunStatus::Completed(code);
            }
            Err(e) => {
                last_exit_code = 1;
                tracing::error!("TUI: Query error: {}", e);
                tui.app.status = crate::tui::RunStatus::Error(e.to_string());
                tui.app.push_error_line(format!("[ERROR] {}", e));
            }
        }
        
        tui.app.pending_qa = false;
        tui.app.qa_started_at = None;
        
        // Phase 4: Wait for user to review results and decide what to do next
        // Create new input reader for review phase
        tracing::debug!("TUI: Waiting for user action (press 'n' for new query, 'q' to quit)");
        let (review_reader, mut review_rx) = InputReader::start();
        let mut review_tick = tokio::time::interval(Duration::from_millis(100));
        
        loop {
            tokio::select! {
                Some(event) = review_rx.recv() => {
                    match event {
                        InputEvent::Key(key) => {
                            use crossterm::event::KeyCode;
                            use crossterm::event::KeyModifiers;
                            match key.code {
                                // 'n' or Enter - start new query
                                KeyCode::Char('n') | KeyCode::Enter => {
                                    tracing::debug!("TUI: Starting new query");
                                    break; // Break inner loop, continue main_loop
                                }
                                // 'q' or Ctrl+C - quit program
                                KeyCode::Char('q') => {
                                    tracing::debug!("TUI: User requested quit");
                                    break 'main_loop;
                                }
                                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                    tracing::debug!("TUI: User requested quit (Ctrl+C)");
                                    break 'main_loop;
                                }
                                // Allow navigation keys to review results
                                _ => {
                                    if tui.app.handle_key(key) {
                                        tracing::debug!("TUI: User requested quit via handle_key");
                                        break 'main_loop;
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
                _ = review_tick.tick() => {}
            }
            
            tui.app.maybe_hide_splash();
            if let Err(e) = tui.terminal.draw(|f| ui::draw(f, &mut tui.app)) {
                tracing::warn!("Render error: {}", e);
            }
        }
        
        // Stop review reader for this cycle
        review_reader.stop();
    }

    // Clean up terminal
    tui.restore();
    
    tracing::debug!("TUI: Exiting with code {}", last_exit_code);
    Ok(last_exit_code)
}

// Modified session runner that reuses the existing input reader
async fn run_tui_session_continuing(
    tui: &mut TuiRuntime,
    input: crate::flow::flow_qa::RunSessionInput,
    input_rx: &mut mpsc::UnboundedReceiver<crate::tui::events::InputEvent>,
    tick: &mut tokio::time::Interval,
) -> Result<RunnerResult, RunnerError> {
    use crate::tui::ui;
    
    tracing::debug!("TUI session (continuing): Starting");
    let (tui_tx, mut tui_rx) = mpsc::unbounded_channel();

    // Unified error handler for execution phase
    let handle_execution_error = |app: &mut TuiApp, error: &str| {
        handle_tui_error(app, error, "ERROR");
    };

    // Set up state monitoring
    if let Some(manager) = input.state_manager.as_ref() {
        let mut state_rx = manager.subscribe();
        let session_id = input.state_session_id.clone();
        let tui_tx_state = tui_tx.clone();
        tokio::spawn(async move {
            let mut phase = RuntimePhase::Initializing;
            let mut memory_hits = 0usize;
            let mut tool_events = 0usize;
            loop {
                match state_rx.recv().await {
                    Ok(event) => {
                        let Some(id) = event.session_id() else { continue; };
                        if session_id.as_deref() != Some(id) {
                            continue;
                        }
                        match event {
                            StateEvent::SessionStateChanged { new_phase, .. } => {
                                phase = new_phase;
                            }
                            StateEvent::ToolEventReceived { event_count, .. } => {
                                tool_events = tool_events.saturating_add(event_count);
                            }
                            StateEvent::MemoryHit { hit_count, .. } => {
                                memory_hits = hit_count;
                            }
                            _ => {}
                        }
                        let _ = tui_tx_state.send(memex_core::tui::TuiEvent::StateUpdate {
                            phase,
                            memory_hits,
                            tool_events,
                        });
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                }
            }
        });
    }

    // Start run task
    let events_out_tx_run = input.events_out_tx.clone();
    let state_manager_run = input.state_manager.clone();
    let state_session_id_run = input.state_session_id.clone();
    let run_id = input.run_id.clone();
    let silent = input.silent;
    let mut run_task = tokio::spawn(async move {
        run_session(
            input.session,
            &input.control,
            input.policy,
            input.capture_bytes,
            events_out_tx_run,
            Some(tui_tx),
            &run_id,
            silent,
            state_manager_run,
            state_session_id_run,
        )
        .await
    });

    let mut run_result: Option<Result<RunnerResult, RunnerError>> = None;
    let mut exit_requested = false;

    // Reuse the SAME event loop with the SAME input_rx!
    tracing::debug!("TUI: Continuing event loop for execution");
    loop {
        tokio::select! {
            Some(event) = tui_rx.recv() => {
                tui.app.handle_event(event);
            }
            Some(event) = input_rx.recv() => {
                use crate::tui::events::InputEvent;
                match event {
                    InputEvent::Key(key) => {
                        // In execution phase, only handle control keys, ignore character input
                        use crossterm::event::KeyCode;
                        match key.code {
                            // Allow only control/navigation keys, ignore character input
                            KeyCode::Char('q') | KeyCode::Char('c') | KeyCode::Tab | 
                            KeyCode::Char('1') | KeyCode::Char('2') | KeyCode::Char('3') |
                            KeyCode::Char('k') | KeyCode::Char('j') | KeyCode::Char('u') | 
                            KeyCode::Char('d') | KeyCode::Char('g') | KeyCode::Char('G') |
                            KeyCode::Char('p') | KeyCode::Char(' ') |
                            KeyCode::Up | KeyCode::Down | KeyCode::PageUp | KeyCode::PageDown => {
                                if tui.app.handle_key(key) {
                                    exit_requested = true;
                                }
                            }
                            _ => {
                                // Ignore other keys during execution (especially character input)
                                tracing::trace!("Ignoring key during execution: {:?}", key);
                            }
                        }
                    }
                    InputEvent::Mouse(_) => {
                        // Ignore mouse events during execution
                    }
                }
            }
            res = &mut run_task => {
                let res = match res {
                    Ok(inner) => inner,
                    Err(e) => {
                        let err_msg = format!("Task panic: {}", e);
                        handle_execution_error(&mut tui.app, &err_msg);
                        run_result = Some(Err(RunnerError::Spawn(err_msg)));
                        continue; // Show error, keep UI running for user to see
                    }
                };
                
                // Process result
                run_result = Some(match res {
                    Ok(result) => {
                        tracing::debug!("Task completed, exit_code={}", result.exit_code);
                        if !tui.app.is_done() {
                            tui.app.status = crate::tui::RunStatus::Completed(result.exit_code);
                        }
                        Ok(result)
                    }
                    Err(err) => {
                        handle_execution_error(&mut tui.app, &err.to_string());
                        // Don't break immediately - let user see the error
                        Err(err)
                    }
                });
                // After setting result, continue rendering to show completion/error state
                // User can press 'q' or Ctrl+C to exit
            }
            _ = tick.tick() => {}
        }

        tui.app.maybe_hide_splash();
        if let Err(e) = tui.terminal.draw(|f| ui::draw(f, &mut tui.app)) {
            // Render errors are non-fatal, just log and continue
            tracing::warn!("Render error (non-fatal): {}", e);
            handle_tui_error(&mut tui.app, &format!("Render error: {}", e), "WARN");
        }
        
        // Exit conditions:
        // 1. User explicitly requested exit (q or Ctrl+C)
        // 2. Run completed AND user pressed a key to exit
        if exit_requested {
            tracing::debug!("TUI: User requested exit");
            break;
        }
        
        // If run is done but user hasn't explicitly exited, keep showing the UI
        // This allows users to review results/errors before exiting
        if tui.app.is_done() && run_result.is_some() {
            tracing::trace!("TUI: Run complete, waiting for user exit (press 'q' or Ctrl+C)");
            // Continue loop to allow user to review and manually exit
        }
    }

    // Return result, defaulting to error if none set
    run_result.unwrap_or_else(|| {
        let err = "Execution ended without result";
        tracing::error!("{}", err);
        Err(RunnerError::Spawn(err.to_string()))
    })
}

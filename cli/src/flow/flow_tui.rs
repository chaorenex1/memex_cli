//! TUI 执行流：单一事件循环处理输入/runner 事件/tick，并支持用户中止（abort）当前运行。
use std::sync::Arc;

use core_api::TuiConfig;
use core_api::{
    EventsOutTx, MemoryPlugin, PolicyPlugin, RunSessionArgs, RunnerError, RunnerEvent,
};
use memex_core::api as core_api;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use uuid::Uuid;

use crate::commands::cli::{Args, RunArgs};
use crate::task_level::infer_task_level;
use crate::tui::{restore_terminal, setup_terminal, TuiApp};
use memex_plugins::plan::{build_runner_spec, PlanMode, PlanRequest};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UiMode {
    Prompt,
    Running,
    Review,
}

pub async fn run_tui_flow(
    args: &Args,
    run_args: Option<&RunArgs>,
    cfg: &mut core_api::AppConfig,
    events_out_tx: Option<EventsOutTx>,
    run_id: String,
    _recover_run_id: Option<String>,
    stream_format: &str,
    policy: Option<Arc<dyn PolicyPlugin>>,
    memory: Option<Arc<dyn MemoryPlugin>>,
    gatekeeper: Arc<dyn core_api::GatekeeperPlugin>,
) -> Result<i32, RunnerError> {
    let mut tui = TuiRuntime::new(&cfg.tui, run_id.clone())?;
    let shared_policy = policy;
    let shared_memory = memory;
    let shared_gatekeeper = gatekeeper;

    use crate::tui::events::{InputEvent, InputReader};
    use crate::tui::ui;
    use std::time::Duration;

    tracing::debug!("TUI: Starting unified event loop");

    let (input_reader, mut input_rx) = InputReader::start();
    let mut tick = tokio::time::interval(Duration::from_millis(
        tui.app.config.update_interval_ms.max(16),
    ));
    let (runner_tx, mut runner_rx) = mpsc::unbounded_channel::<RunnerEvent>();
    let mut mode = UiMode::Prompt;
    let mut run_done_rx: Option<oneshot::Receiver<Result<i32, RunnerError>>> = None;
    let mut abort_tx: Option<mpsc::Sender<String>> = None;
    let mut last_exit_code = 0;

    tui.app.reset_for_new_query();
    tui.app.set_prompt_mode();

    loop {
        tokio::select! {
            Some(event) = input_rx.recv() => {
                match mode {
                    UiMode::Prompt => match event {
                        InputEvent::Key(key) => {
                            use crate::tui::PromptAction;
                            match tui.app.handle_prompt_key(key) {
                                PromptAction::Submit => {
                                    let user_input = tui.app.input_buffer.trim().to_string();
                                    if user_input.is_empty() {
                                        tui.app.push_error_line("[WARN] empty prompt".into());
                                    } else {
                                        let cfg_snapshot = cfg.clone();
                                        let capture_bytes = args.capture_bytes;

                                        // Transition to running
                                        tui.app.input_buffer.clear();
                                        tui.app.input_cursor = 0;
                                        tui.app.input_mode = crate::tui::InputMode::Normal;
                                        tui.app.pending_qa = false;
                                        tui.app.qa_started_at = None;

                                        let query_run_id = Uuid::new_v4().to_string();
                                        tui.app.run_id = query_run_id.clone();
                                        tui.app.status = crate::tui::RunStatus::Running;

                                        let plan_req = build_plan_request(args, run_args, stream_format);
                                        let (runner_spec, start_data) = build_runner_spec(cfg, plan_req)?;

                                        let query_policy = shared_policy.clone();
                                        let query_memory = shared_memory.clone();
                                        let query_gatekeeper = shared_gatekeeper.clone();
                                        let events_out_tx = events_out_tx.clone();
                                        let runner_tx = runner_tx.clone();
                                        let stream_format = stream_format.to_string();
                                        let (new_abort_tx, abort_rx) = mpsc::channel::<String>(1);
                                        abort_tx = Some(new_abort_tx);

                                        let (done_tx, done_rx) = oneshot::channel();
                                        run_done_rx = Some(done_rx);
                                        mode = UiMode::Running;

                                        tokio::spawn(async move {
                                            let res = core_api::run_with_query(
                                                core_api::RunWithQueryArgs {
                                                    user_query: user_input,
                                                    cfg: cfg_snapshot,
                                                    runner: runner_spec,
                                                    run_id: query_run_id,
                                                    capture_bytes,
                                                    stream_format,
                                                    events_out_tx,
                                                    policy: query_policy,
                                                    memory: query_memory,
                                                    gatekeeper: query_gatekeeper,
                                                    wrapper_start_data: start_data,
                                                },
                                                |input| async move {
                                                    core_api::run_session(RunSessionArgs {
                                                        session: input.session,
                                                        control: &input.control,
                                                        policy: input.policy,
                                                        capture_bytes: input.capture_bytes,
                                                        events_out: input.events_out_tx,
                                                        event_tx: Some(runner_tx),
                                                        run_id: &input.run_id,
                                                        backend_kind: &input.backend_kind,
                                                        stream_format: &input.stream_format,
                                                        abort_rx: Some(abort_rx),
                                                    })
                                                    .await
                                                },
                                            )
                                            .await;
                                            let _ = done_tx.send(res);
                                        });
                                    }
                                }
                                PromptAction::Clear => {
                                    tui.app.input_buffer.clear();
                                    tui.app.input_cursor = 0;
                                    tui.app.clear_selection();
                                }
                                PromptAction::Exit => {
                                    break;
                                }
                                PromptAction::None => {}
                            }
                        }
                        InputEvent::Mouse(mouse) => {
                            let area = tui.terminal.get_frame().area();
                            tui.app.handle_mouse(mouse, area);
                        }
                    },
                    UiMode::Running => {
                        if let InputEvent::Key(key) = event {
                            use crossterm::event::KeyCode;
                            use crossterm::event::KeyModifiers;
                            match key.code {
                                // allow navigation / pause, but do not allow quitting mid-run
                                KeyCode::Tab | KeyCode::Char('1') | KeyCode::Char('2') | KeyCode::Char('3') |
                                KeyCode::Char('k') | KeyCode::Char('j') | KeyCode::Char('u') | KeyCode::Char('d') |
                                KeyCode::Char('g') | KeyCode::Char('G') | KeyCode::Char('p') | KeyCode::Char(' ') |
                                KeyCode::Up | KeyCode::Down | KeyCode::PageUp | KeyCode::PageDown => {
                                    let _ = tui.app.handle_key(key);
                                }
                                KeyCode::Char('q') | KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                    if let Some(tx) = abort_tx.as_ref() {
                                        let _ = tx.try_send("user requested abort".into());
                                    }
                                    tui.app.push_error_line("[INFO] abort requested".into());
                                }
                                _ => {}
                            }
                        }
                    }
                    UiMode::Review => {
                        if let InputEvent::Key(key) = event {
                            use crossterm::event::KeyCode;
                            use crossterm::event::KeyModifiers;
                            match key.code {
                                KeyCode::Char('n') | KeyCode::Enter => {
                                    tui.app.reset_for_new_query();
                                    tui.app.set_prompt_mode();
                                    mode = UiMode::Prompt;
                                }
                                KeyCode::Char('q') => break,
                                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => break,
                                _ => {
                                    let _ = tui.app.handle_key(key);
                                }
                            }
                        }
                    }
                }
            }

            Some(event) = runner_rx.recv() => {
                tui.app.handle_event(event);
            }

            res = async {
                match run_done_rx.as_mut() {
                    Some(rx) => rx.await,
                    None => std::future::pending().await,
                }
            } => {
                run_done_rx = None;
                abort_tx = None;
                mode = UiMode::Review;

                let result = match res {
                    Ok(r) => r,
                    Err(_) => Err(RunnerError::Spawn("run task canceled".into())),
                };
                match result {
                    Ok(code) => {
                        last_exit_code = code;
                        tui.app.status = crate::tui::RunStatus::Completed(code);
                    }
                    Err(e) => {
                        last_exit_code = 1;
                        tui.app.status = crate::tui::RunStatus::Error(e.to_string());
                        tui.app.push_error_line(format!("[ERROR] {}", e));
                    }
                }
            }

            _ = tick.tick() => {}
        }

        tui.app.maybe_hide_splash();
        if let Err(e) = tui.terminal.draw(|f| ui::draw(f, &tui.app)) {
            handle_tui_error(&mut tui.app, &format!("Render error: {}", e), "WARN");
        }
    }

    input_reader.stop();

    // Clean up terminal
    tui.restore();

    tracing::debug!("TUI: Exiting with code {}", last_exit_code);
    Ok(last_exit_code)
}

fn build_plan_request(
    args: &Args,
    run_args: Option<&RunArgs>,
    stream_format: &str,
) -> PlanRequest {
    let mode = match run_args {
        Some(ra) => {
            let backend_kind = ra.backend_kind.map(|kind| match kind {
                crate::commands::cli::BackendKind::Codecli => "codecli".to_string(),
                crate::commands::cli::BackendKind::Aiservice => "aiservice".to_string(),
            });

            let task_level = match ra.task_level {
                crate::commands::cli::TaskLevel::Auto => {
                    let prompt_for_level = ra
                        .prompt
                        .clone()
                        .unwrap_or_else(|| args.codecli_args.join(" "));
                    format!("{:?}", infer_task_level(&prompt_for_level))
                }
                lv => format!("{lv:?}"),
            };

            PlanMode::Backend {
                backend_spec: ra.backend.clone(),
                backend_kind,
                env_file: ra.env_file.clone(),
                env: ra.env.clone(),
                model: ra.model.clone(),
                task_level: Some(task_level),
            }
        }
        None => PlanMode::Legacy {
            cmd: args.codecli_bin.clone(),
            args: args.codecli_args.clone(),
        },
    };

    PlanRequest {
        mode,
        resume_id: None,
        stream_format: stream_format.to_string(),
    }
}

use clap::Parser;
mod app;
mod commands;
mod tui;
use commands::cli;
use memex_core::context::AppContext;
use memex_core::error;
use memex_core::replay;
use memex_core::state::{StateEvent, StateManager};
use std::sync::Arc;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

static LOG_GUARD: std::sync::OnceLock<tracing_appender::non_blocking::WorkerGuard> =
    std::sync::OnceLock::new();

#[tokio::main]
async fn main() {
    let exit = match real_main().await {
        Ok(code) => code,
        Err(e) => {
            eprintln!("{e}");
            exit_code_for_error(&e)
        }
    };

    std::process::exit(exit);
}

async fn real_main() -> Result<i32, error::CliError> {
    let mut args = cli::Args::parse();
    let cfg =
        memex_core::config::load_default().map_err(|e| error::CliError::Config(e.to_string()))?;
    init_tracing(&cfg.logging).map_err(error::CliError::Command)?;

    let env_state_enabled = std::env::var("MEMEX_ENABLE_STATE_MGMT")
        .ok()
        .map(|v| v.eq_ignore_ascii_case("true"));
    let state_enabled = env_state_enabled.unwrap_or(cfg.state_management.enabled);

    let state_manager = if state_enabled {
        Some(Arc::new(StateManager::new()))
    } else {
        None
    };
    if let Some(manager) = state_manager.as_ref() {
        let mut event_rx = manager.subscribe();
        tokio::spawn(async move {
            while let Ok(event) = event_rx.recv().await {
                match event {
                    StateEvent::SessionCreated { session_id, .. } => {
                        tracing::debug!("Session created: {}", session_id);
                    }
                    StateEvent::SessionStateChanged {
                        session_id,
                        new_phase,
                        ..
                    } => {
                        tracing::debug!("Session {} -> {:?}", session_id, new_phase);
                    }
                    StateEvent::SessionCompleted {
                        session_id,
                        exit_code,
                        duration_ms,
                        ..
                    } => {
                        tracing::info!(
                            "Session {} completed (exit={}, {}ms)",
                            session_id,
                            exit_code,
                            duration_ms
                        );
                    }
                    StateEvent::SessionFailed {
                        session_id, error, ..
                    } => {
                        tracing::error!("Session {} failed: {}", session_id, error);
                    }
                    _ => {}
                }
            }
        });
    }

    let ctx = AppContext::new(cfg, state_manager)
        .await
        .map_err(error::CliError::Runner)?;

    let cmd = args.command.take();

    if let Some(cmd) = cmd {
        return dispatch(cmd, args, ctx).await;
    }

    let exit = app::run_app_with_config(args, None, None, &ctx).await?;
    Ok(exit)
}

fn exit_code_for_error(e: &error::CliError) -> i32 {
    // 0: success
    // 11: config error
    // 20: runner start / IO error
    // 40: policy deny (usually returned as a normal exit code, not as an error)
    // 50: internal/uncategorized
    match e {
        error::CliError::Config(_) => 11,
        error::CliError::Runner(re) => match re {
            error::RunnerError::Config(_) => 11,
            error::RunnerError::Spawn(_) => 20,
            error::RunnerError::StreamIo { .. } => 20,
            error::RunnerError::Plugin(_) => 50,
        },
        error::CliError::Io(_) => 20,
        error::CliError::Command(_) => 20,
        error::CliError::Replay(_) => 50,
        error::CliError::Anyhow(_) => 50,
    }
}

async fn dispatch(
    cmd: cli::Commands,
    args: cli::Args,
    ctx: AppContext,
) -> Result<i32, error::CliError> {
    match cmd {
        cli::Commands::Run(run_args) => {
            let exit = app::run_app_with_config(args, Some(run_args), None, &ctx).await?;
            Ok(exit)
        }
        cli::Commands::Replay(replay_args) => {
            let core_args = replay::ReplayArgs {
                events: replay_args.events,
                run_id: replay_args.run_id,
                format: replay_args.format,
                set: replay_args.set,
                rerun_gatekeeper: replay_args.rerun_gatekeeper,
            };
            replay::replay_cmd(core_args).map_err(error::CliError::Replay)?;
            Ok(0)
        }
        cli::Commands::Resume(resume_args) => {
            let recover_id = Some(resume_args.run_id.clone());
            let exit = app::run_app_with_config(args, Some(resume_args.run_args), recover_id, &ctx)
                .await?;
            Ok(exit)
        }
    }
}

fn init_tracing(logging: &memex_core::config::LoggingConfig) -> Result<(), String> {
    if !logging.enabled {
        return Ok(());
    }

    let filter = match std::env::var("RUST_LOG") {
        Ok(v) if !v.trim().is_empty() => EnvFilter::from_default_env(),
        _ => EnvFilter::try_new(logging.level.clone()).map_err(|e| e.to_string())?,
    };

    let mut maybe_writer = None;

    if logging.file {
        let dir = match logging
            .directory
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            Some(d) => std::path::PathBuf::from(d),
            None => std::env::temp_dir().join("memex-cli"),
        };

        std::fs::create_dir_all(&dir).map_err(|e| format!("create log dir failed: {e}"))?;
        let file_name = format!("memex-cli.{}.log", std::process::id());
        let appender = tracing_appender::rolling::never(dir, file_name);
        let (non_blocking, guard) = tracing_appender::non_blocking(appender);
        let _ = LOG_GUARD.set(guard);
        maybe_writer = Some(non_blocking);
    }

    if !logging.console && maybe_writer.is_none() {
        return Err("logging disabled for both console and file".to_string());
    }

    let console_layer = logging.console.then(|| {
        tracing_subscriber::fmt::layer()
            .with_writer(std::io::stderr)
            .with_ansi(atty::is(atty::Stream::Stderr))
    });

    let file_layer = maybe_writer.map(|w| {
        tracing_subscriber::fmt::layer()
            .with_writer(w)
            .with_ansi(false)
    });

    tracing_subscriber::registry()
        .with(filter)
        .with(console_layer)
        .with(file_layer)
        .init();

    Ok(())
}

use crate::commands::cli::{Args, RunArgs};
use memex_core::config::MemoryProvider;
use memex_core::context::AppContext;
use memex_core::error::RunnerError;
use memex_plugins::factory;

use crate::flow::{standard, tui};

#[tracing::instrument(name = "cli.run_app", skip(args, run_args, ctx))]
pub async fn run_app_with_config(
    args: Args,
    run_args: Option<RunArgs>,
    recover_run_id: Option<String>,
    ctx: &AppContext,
) -> Result<i32, RunnerError> {
    let args = args;
    let mut cfg = ctx.cfg().clone();
    let state_manager = ctx.state_manager();

    let force_tui = run_args.as_ref().map(|ra| ra.tui).unwrap_or(false);

    if let Some(ra) = &run_args {
        if let Some(pid) = &ra.project_id {
            cfg.project_id = pid.clone();
        }

        if let Some(url) = &ra.memory_base_url {
            let MemoryProvider::Service(ref mut svc_cfg) = cfg.memory.provider;
            svc_cfg.base_url = url.clone();
        }
        if let Some(key) = &ra.memory_api_key {
            let MemoryProvider::Service(ref mut svc_cfg) = cfg.memory.provider;
            svc_cfg.api_key = key.clone();
        }
    }

    let mut stream_format = run_args
        .as_ref()
        .map(|ra| ra.stream_format.clone())
        .unwrap_or_else(|| "text".to_string());
    let mut stream_enabled = run_args.as_ref().map(|ra| ra.stream).unwrap_or(false);

    if force_tui {
        stream_enabled = true;
        if stream_format != "text" {
            stream_format = "text".to_string();
        }
    }

    let stream = factory::build_stream(&stream_format);
    let stream_plan = stream.apply(&mut cfg);
    let mut should_use_tui = force_tui;
    tracing::info!("TUI force enabled: {}", force_tui);
    tracing::info!("TUI config enabled: {}", cfg.tui.enabled);
    if !cfg.tui.enabled {
        should_use_tui = false;
    }
    if should_use_tui {
        if let Err(reason) = crate::tui::check_tui_support() {
            tracing::debug!("TUI disabled: {}", reason);
            should_use_tui = false;
        }
    }

    let events_out_tx = ctx.events_out();

    let memory = factory::build_memory(&cfg).map_err(|e| RunnerError::Spawn(e.to_string()))?;
    let policy = factory::build_policy(&cfg);
    let gatekeeper = factory::build_gatekeeper(&cfg);

    let run_id = recover_run_id
        .clone()
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    tracing::debug!(run_id = %run_id, stream_format = %stream_format, "run initialized");

    if should_use_tui {
        return tui::run_tui_flow(
            &args,
            run_args.as_ref(),
            &mut cfg,
            state_manager.clone(),
            events_out_tx,
            run_id,
            recover_run_id.clone(),
            stream_enabled,
            &stream_format,
            stream_plan.silent,
            policy,
            memory,
            gatekeeper,
        )
        .await;
    } else {
        standard::run_standard_flow(
            &args,
            run_args.as_ref(),
            &mut cfg,
            state_manager.clone(),
            events_out_tx,
            run_id,
            recover_run_id.clone(),
            stream_enabled,
            &stream_format,
            stream_plan.silent,
            policy,
            memory,
            gatekeeper,
        )
        .await
    }
}

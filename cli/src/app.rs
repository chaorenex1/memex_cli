//! CLI 应用装配层：合并配置覆盖、构建 services/stream，并在 Standard/TUI flow 之间分发。
use crate::commands::cli::{Args, RunArgs};
use memex_core::api as core_api;

use crate::flow::{standard, tui};

#[tracing::instrument(name = "cli.run_app", skip(args, run_args, ctx))]
pub async fn run_app_with_config(
    args: Args,
    run_args: Option<RunArgs>,
    recover_run_id: Option<String>,
    ctx: &core_api::AppContext,
) -> Result<i32, core_api::RunnerError> {
    let args = args;
    let mut cfg = ctx.cfg().clone();

    let force_tui = run_args.as_ref().map(|ra| ra.tui).unwrap_or(false);

    if let Some(ra) = &run_args {
        if let Some(url) = &ra.memory_base_url {
            let core_api::MemoryProvider::Service(ref mut svc_cfg) = cfg.memory.provider;
            svc_cfg.base_url = url.clone();
        }
        if let Some(key) = &ra.memory_api_key {
            let core_api::MemoryProvider::Service(ref mut svc_cfg) = cfg.memory.provider;
            svc_cfg.api_key = key.clone();
        }
    }
    let project_id =
        if let Some(project_id) = run_args.as_ref().and_then(|ra| ra.project_id.clone()) {
            project_id
        } else {
            std::env::current_dir()
                .map_err(|e| {
                    core_api::RunnerError::Config(format!(
                        "failed to determine project_id from current_dir fallback: {e}"
                    ))
                })?
                .to_string_lossy()
                .to_string()
        };

    let stream_format = run_args
        .as_ref()
        .map(|ra| ra.stream_format.clone())
        .unwrap_or_else(|| "text".to_string());
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

    let services = ctx.build_services(&cfg)?;

    let run_id = recover_run_id
        .clone()
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    tracing::debug!(run_id = %run_id, stream_format = %stream_format, "run initialized");

    if should_use_tui {
        return tui::run_tui_flow(
            &args,
            run_args.as_ref(),
            &mut cfg,
            events_out_tx,
            run_id,
            recover_run_id.clone(),
            &stream_format,
            &project_id,
            &services,
        )
        .await;
    } else {
        standard::run_standard_flow(
            &args,
            run_args.as_ref(),
            &mut cfg,
            events_out_tx,
            run_id,
            recover_run_id.clone(),
            &stream_format,
            &project_id,
            &services,
        )
        .await
    }
}

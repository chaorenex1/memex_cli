//! CLI 应用装配层：合并配置覆盖、构建 services/stream，并在 Standard/TUI flow 之间分发。
use crate::commands::cli::{Args, RunArgs};
use memex_core::api as core_api;

use crate::flow::standard;

#[tracing::instrument(name = "cli.run_app", skip(args, run_args, ctx))]
pub async fn run_app_with_config(
    args: Args,
    run_args: Option<RunArgs>,
    recover_run_id: Option<String>,
    is_remote: &bool,
    ctx: &core_api::AppContext,
) -> Result<i32, core_api::RunnerError> {
    let args = args;
    let cfg = ctx.cfg().clone();

    let force_tui = run_args.as_ref().map(|ra| ra.tui).unwrap_or(false);
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

    if should_use_tui {
        tracing::warn!("TUI disabled!");
        // return tui::run_tui_flow(
        //     &args,
        //     run_args.as_ref(),
        //     &mut cfg,
        //     events_out_tx,
        //     run_id,
        //     recover_run_id.clone(),
        //     &stream_format,
        //     &project_id,
        //     &services,
        // )
        // .await;
        Ok(0)
    } else {
        standard::run_standard_flow(
            &args,
            run_args.as_ref(),
            ctx,
            is_remote,
            recover_run_id.clone(),
        )
        .await
    }
}

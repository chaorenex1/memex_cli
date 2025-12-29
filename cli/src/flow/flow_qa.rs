use memex_core::config::AppConfig;
use memex_core::engine::RunnerSpec;
use memex_core::error::RunnerError;
use memex_core::runner::{RunnerPlugin, RunnerStartArgs};
use memex_plugins::factory;

use crate::commands::cli::{Args, BackendKind, RunArgs, TaskLevel};
use crate::utils::parse_env_file;

pub fn infer_task_level(prompt: &str) -> TaskLevel {
    let s = prompt.trim();
    if s.is_empty() {
        return TaskLevel::L1;
    }

    let lower = s.to_ascii_lowercase();

    // Strong engineering / multi-step signals => L2
    if lower.contains("architecture")
        || lower.contains("debug")
        || lower.contains("refactor")
        || lower.contains("compile")
        || lower.contains("cargo")
        || lower.contains("stack trace")
        || lower.contains("benchmark")
        || s.contains("```")
    {
        return TaskLevel::L2;
    }

    // High creativity / style-heavy signals => L3
    if lower.contains("story")
        || lower.contains("novel")
        || lower.contains("brand")
        || lower.contains("marketing")
        || lower.contains("style")
    {
        return TaskLevel::L3;
    }

    // Very short tool-like requests => L0
    if s.chars().count() <= 200
        && (lower.contains("translate")
            || lower.contains("format")
            || lower.contains("json")
            || lower.contains("rewrite"))
    {
        return TaskLevel::L0;
    }

    TaskLevel::L1
}

pub fn build_runner_spec(
    args: &Args,
    run_args: Option<&RunArgs>,
    cfg: &mut AppConfig,
    recover_run_id: Option<String>,
    stream_enabled: bool,
    stream_format: &str,
) -> Result<(RunnerSpec, Option<serde_json::Value>), RunnerError> {
    let mut base_envs: std::collections::HashMap<String, String> = std::env::vars().collect();

    let (spec, start_data) = if let Some(ra) = run_args {
        let backend_spec = ra.backend.clone();
        let backend_kind = ra.backend_kind.map(|kind| match kind {
            BackendKind::Codecli => "codecli",
            BackendKind::Aiservice => "aiservice",
        });
        if let Some(kind) = backend_kind {
            cfg.backend_kind = kind.to_string();
        }

        let backend = match backend_kind {
            Some(kind) => factory::build_backend_with_kind(kind, &backend_spec),
            None => factory::build_backend(&backend_spec),
        };

        if let Some(path) = &ra.env_file {
            let file_envs = parse_env_file(path)?;
            for (k, v) in file_envs {
                base_envs.insert(k, v);
            }
        }

        // Merge extra envs from CLI flags (KEY=VALUE), overriding process env.
        for kv in ra.env.iter() {
            if let Some((k, v)) = kv.split_once('=') {
                if !k.trim().is_empty() {
                    base_envs.insert(k.trim().to_string(), v.to_string());
                }
            }
        }

        let task_level = match ra.task_level {
            TaskLevel::Auto => {
                let prompt_for_level = ra
                    .prompt
                    .clone()
                    .unwrap_or_else(|| args.codecli_args.join(" "));
                infer_task_level(&prompt_for_level)
            }
            lv => lv,
        };

        (
            RunnerSpec::Backend {
                strategy: backend,
                backend_spec,
                base_envs,
                resume_id: recover_run_id,
                model: ra.model.clone(),
                stream: stream_enabled,
                stream_format: stream_format.to_string(),
            },
            Some(serde_json::json!({ "task_level": format!("{task_level:?}") })),
        )
    } else {
        // Legacy mode (no subcommand): passthrough cmd/args exactly as provided.
        let runner: Box<dyn RunnerPlugin> = factory::build_runner(cfg);
        let session_args = RunnerStartArgs {
            cmd: args.codecli_bin.clone(),
            args: args.codecli_args.clone(),
            envs: base_envs,
        };
        (
            RunnerSpec::Passthrough { runner, session_args },
            None,
        )
    };

    Ok((spec, start_data))
}

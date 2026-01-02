//! Planner：把 CLI 参数与配置合并为可执行的 `RunnerSpec`（backend/legacy），并生成 wrapper start data。
use std::collections::HashMap;

use memex_core::api as core_api;

use crate::factory;

pub enum PlanMode {
    Backend {
        backend_spec: String,
        backend_kind: Option<String>,
        env_file: Option<String>,
        env: Vec<String>,
        model: Option<String>,
        model_provider: Option<String>,
        project_id: Option<String>,
        task_level: Option<String>,
    },
    Legacy {
        cmd: String,
        args: Vec<String>,
    },
}

pub struct PlanRequest {
    pub mode: PlanMode,
    pub resume_id: Option<String>,
    pub stream_format: String,
}

pub fn build_runner_spec(
    cfg: &mut core_api::AppConfig,
    req: PlanRequest,
) -> Result<(core_api::RunnerSpec, Option<serde_json::Value>), core_api::RunnerError> {
    // 初始化 base_envs 时继承当前进程的环境变量（特别是 PATH）
    let mut base_envs: HashMap<String, String> = std::env::vars().collect();

    match req.mode {
        PlanMode::Backend {
            backend_spec,
            backend_kind,
            env_file,
            env,
            model,
            model_provider,
            project_id,
            task_level,
        } => {
            if let Some(kind) = backend_kind.as_deref() {
                if !kind.trim().is_empty() {
                    cfg.backend_kind = kind.to_string();
                }
            }

            // Merge envs from config dir .env file.
            let file_envs = parse_env_file(&cfg.env_file)?;
            for (k, v) in file_envs {
                base_envs.insert(k, v);
            }

            if let Some(path) = env_file.as_deref() {
                let file_envs = parse_env_file(path)?;
                for (k, v) in file_envs {
                    base_envs.insert(k, v);
                }
            }

            // Merge extra envs from CLI flags (KEY=VALUE), overriding process env.
            for kv in env.iter() {
                if let Some((k, v)) = kv.split_once('=') {
                    if !k.trim().is_empty() {
                        base_envs.insert(k.trim().to_string(), v.to_string());
                    }
                }
            }

            let backend = match backend_kind.as_deref() {
                Some(kind) => factory::build_backend_with_kind(kind, &backend_spec),
                None => factory::build_backend(&backend_spec),
            };

            let start_data = task_level.map(|lv| serde_json::json!({ "task_level": lv }));

            Ok((
                core_api::RunnerSpec::Backend {
                    strategy: backend,
                    backend_spec,
                    base_envs,
                    resume_id: req.resume_id,
                    model,
                    stream_format: req.stream_format,
                    model_provider,
                    project_id,
                },
                start_data,
            ))
        }
        PlanMode::Legacy { cmd, args } => {
            let runner: Box<dyn core_api::RunnerPlugin> = factory::build_runner(cfg);
            let session_args = core_api::RunnerStartArgs {
                cmd,
                args,
                envs: base_envs,
            };
            Ok((
                core_api::RunnerSpec::Passthrough {
                    runner,
                    session_args,
                },
                None,
            ))
        }
    }
}

fn parse_env_file(path: &str) -> Result<Vec<(String, String)>, core_api::RunnerError> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| core_api::RunnerError::Spawn(format!("failed to read env file: {}", e)))?;
    let mut out = Vec::new();

    for (idx, raw_line) in content.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() {
            return Err(core_api::RunnerError::Spawn(format!(
                "env file contains empty line at {}",
                idx + 1
            )));
        }
        if line.starts_with('#') {
            continue;
        }
        let (k, v) = line.split_once('=').ok_or_else(|| {
            core_api::RunnerError::Spawn(format!(
                "invalid env line at {} (expected KEY=VALUE)",
                idx + 1
            ))
        })?;
        let key = k.trim();
        if key.is_empty() {
            return Err(core_api::RunnerError::Spawn(format!(
                "invalid env line at {} (empty key)",
                idx + 1
            )));
        }
        let value = parse_env_value(v.trim(), idx + 1)?;
        out.push((key.to_string(), value));
    }

    Ok(out)
}

fn parse_env_value(value: &str, line_no: usize) -> Result<String, core_api::RunnerError> {
    if value.len() >= 2 {
        let first = value.chars().next().unwrap();
        let last = value.chars().last().unwrap();
        if (first == '"' && last == '"') || (first == '\'' && last == '\'') {
            let inner = &value[1..value.len() - 1];
            return unescape_env_value(inner, line_no);
        }
    }
    Ok(value.to_string())
}

fn unescape_env_value(value: &str, line_no: usize) -> Result<String, core_api::RunnerError> {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        let Some(next) = chars.next() else {
            return Err(core_api::RunnerError::Spawn(format!(
                "invalid escape at line {} (trailing backslash)",
                line_no
            )));
        };
        match next {
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            't' => out.push('\t'),
            '\\' => out.push('\\'),
            '"' => out.push('"'),
            '\'' => out.push('\''),
            other => out.push(other),
        }
    }
    Ok(out)
}

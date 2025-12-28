use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;

use memex_core::backend::{BackendPlan, BackendStrategy};
use memex_core::runner::RunnerStartArgs;

use crate::runner::codecli::CodeCliRunnerPlugin;

pub struct CodeCliBackendStrategy;

impl BackendStrategy for CodeCliBackendStrategy {
    fn name(&self) -> &str {
        "codecli"
    }

    fn plan(
        &self,
        backend: &str,
        base_envs: HashMap<String, String>,
        resume_id: Option<String>,
        prompt: String,
        model: Option<String>,
        stream: bool,
        stream_format: &str,
    ) -> Result<BackendPlan> {
        let mut args: Vec<String> = Vec::new();

        let exe = backend_basename_lower(backend);
        let want_stream_json = stream_format == "jsonl";

        if exe.contains("codex") {
            // Matches examples like: codex exec "..." --json
            args.push("exec".to_string());

            if let Some(m) = &model {
                args.push("--model".to_string());
                args.push(m.clone());
            }

            if want_stream_json {
                args.push("--json".to_string());
            }

            // Resume: codex exec [--json] resume <id> <prompt>
            if let Some(resume_id) = resume_id.as_deref() {
                if !resume_id.trim().is_empty() {
                    args.push("resume".to_string());
                    args.push(resume_id.to_string());
                }
            }

            if !prompt.is_empty() {
                args.push(prompt);
            }
        } else if exe.contains("claude") {
            // Matches examples like:
            // claude "..." -p --output-format stream-json --verbose
            if !prompt.is_empty() {
                args.push(prompt);
            }

            if stream || want_stream_json {
                args.push("-p".to_string());
            }

            if want_stream_json {
                args.push("--output-format".to_string());
                args.push("stream-json".to_string());
            }

            if let Some(m) = &model {
                args.push("--model".to_string());
                args.push(m.clone());
            }

            // Resume: -r <id>
            if let Some(resume_id) = resume_id.as_deref() {
                if !resume_id.trim().is_empty() {
                    args.push("-r".to_string());
                    args.push(resume_id.to_string());
                }
            }
        } else if exe.contains("gemini") {
            // Matches examples like:
            // gemini -p "..." -y -o stream-json
            if !prompt.is_empty() {
                args.push("-p".to_string());
                args.push(prompt);
            }

            if want_stream_json {
                args.push("-o".to_string());
                args.push("stream-json".to_string());
            }

            // Resume: -r <id> (e.g. -r latest)
            if let Some(resume_id) = resume_id.as_deref() {
                if !resume_id.trim().is_empty() {
                    args.push("-r".to_string());
                    args.push(resume_id.to_string());
                }
            }

            // Leave -y (YOLO) and auth concerns to the user's environment.
            if let Some(m) = &model {
                args.push("--model".to_string());
                args.push(m.clone());
            }
        } else {
            // Generic passthrough-ish fallback (previous behavior).
            if let Some(m) = model {
                args.push("--model".to_string());
                args.push(m);
            }
            if stream {
                args.push("--stream".to_string());
            }
            if !prompt.is_empty() {
                args.push(prompt);
            }
        }

        Ok(BackendPlan {
            runner: Box::new(CodeCliRunnerPlugin::new()),
            session_args: RunnerStartArgs {
                cmd: backend.to_string(),
                args,
                envs: base_envs,
            },
        })
    }
}

fn backend_basename_lower(backend: &str) -> String {
    let p = Path::new(backend);
    let s = p.file_stem().and_then(|x| x.to_str()).unwrap_or(backend);
    s.to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn envs() -> HashMap<String, String> {
        HashMap::new()
    }

    #[test]
    fn codex_resume_maps_to_subcommand() {
        let strat = CodeCliBackendStrategy;
        let plan = strat
            .plan(
                "codex",
                envs(),
                Some("sess-123".to_string()),
                "hi".to_string(),
                None,
                true,
                "jsonl",
            )
            .unwrap();

        assert!(plan.session_args.args.contains(&"resume".to_string()));
        assert!(plan.session_args.args.contains(&"sess-123".to_string()));
    }

    #[test]
    fn claude_resume_maps_to_r_flag() {
        let strat = CodeCliBackendStrategy;
        let plan = strat
            .plan(
                "claude",
                envs(),
                Some("sess-abc".to_string()),
                "hi".to_string(),
                None,
                true,
                "jsonl",
            )
            .unwrap();

        let idx = plan
            .session_args
            .args
            .iter()
            .position(|a| a == "-r")
            .unwrap();
        assert_eq!(plan.session_args.args[idx + 1], "sess-abc");
    }

    #[test]
    fn gemini_resume_maps_to_r_flag() {
        let strat = CodeCliBackendStrategy;
        let plan = strat
            .plan(
                "gemini",
                envs(),
                Some("latest".to_string()),
                "hi".to_string(),
                None,
                true,
                "jsonl",
            )
            .unwrap();

        let idx = plan
            .session_args
            .args
            .iter()
            .position(|a| a == "-r")
            .unwrap();
        assert_eq!(plan.session_args.args[idx + 1], "latest");
    }
}

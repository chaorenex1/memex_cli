use anyhow::Result;

use memex_core::api as core_api;

use crate::backend::encoding::{
    detect_encoding_strategy, escape_shell_arg, prepare_stdin_payload, EncodingStrategy,
};
use crate::runner::codecli::CodeCliRunnerPlugin;

pub struct CodeCliBackendStrategy;

impl core_api::BackendStrategy for CodeCliBackendStrategy {
    fn name(&self) -> &str {
        "codecli"
    }

    fn plan(&self, request: core_api::BackendPlanRequest) -> Result<core_api::BackendPlan> {
        let core_api::BackendPlanRequest {
            backend,
            base_envs,
            resume_id,
            prompt: raw_prompt,
            model,
            model_provider,
            project_id,
            stream_format,
        } = request;

        // 提取命令类型用于判断参数格式（codex/claude/gemini）
        let cmd_type = extract_command_type(&backend);

        // 使用新的编码策略检测
        let encoding_strategy = detect_encoding_strategy(&raw_prompt);
        let use_stdin_prompt = match encoding_strategy {
            EncodingStrategy::DirectArgs => false,
            EncodingStrategy::ForceStdin { .. } => {
                // 仅对支持 stdin 的后端启用
                cmd_type.contains("codex")
                    || cmd_type.contains("gemini")
                    || cmd_type.contains("claude")
            }
        };

        let stdin_payload = if use_stdin_prompt {
            Some(prepare_stdin_payload(&raw_prompt))
        } else {
            None
        };

        tracing::info!(
            "Encoding strategy: {:?}, prompt_len: {}, use_stdin: {}",
            encoding_strategy,
            raw_prompt.len(),
            use_stdin_prompt
        );

        let mut args: Vec<String> = Vec::new();
        let envs = base_envs;
        tracing::info!(
            "Preparing CodeCLI backend plan with backend: {}, model: {:?}, resume_id: {:?}, stream_format: {}",
            backend,
            model,
            resume_id,
            stream_format
        );

        // 解析可执行文件完整路径
        let exe_path = resolve_executable_path(&backend)?;
        tracing::info!("Resolved executable path: {}", exe_path);

        let cwd = if !cmd_type.contains("codex") {
            project_id
                .as_deref()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
        } else {
            None
        };

        if cmd_type.contains("codex") {
            // Matches examples like: codex exec "..." --json
            args.push("exec".to_string());
            args.push("--skip-git-repo-check".to_string());
            if let Some(m) = &model {
                if !m.trim().is_empty() {
                    args.push("--model".to_string());
                    args.push(m.clone());
                }
            }

            if let Some(provider) = &model_provider {
                args.push("--oss".to_string());
                args.push("--local-provider".to_string());
                args.push(provider.clone());
            }

            args.push("--json".to_string());

            if let Some(dir) = &project_id {
                args.push("--cd".to_string());
                args.push(dir.clone());
            }

            // Resume: codex exec [--json] resume <id> <prompt>
            if let Some(resume_id) = resume_id.as_deref() {
                if !resume_id.trim().is_empty() {
                    args.push("resume".to_string());
                    args.push(resume_id.to_string());
                }
            }

            if !raw_prompt.is_empty() && !use_stdin_prompt {
                args.push(escape_shell_arg(&raw_prompt));
            }
        } else if cmd_type.contains("claude") {
            // Matches examples like:
            // claude "..." -p --output-format stream-json --verbose

            // Claude CLI expects the prompt as the first positional argument in common usage.
            // If we place the prompt after flags, it may fall back to interactive mode and appear to hang.

            // Always use non-interactive output mode for the wrapper.
            args.push("-p".to_string());
            args.push("--dangerously-skip-permissions".to_string());
            args.push("--setting-sources=".to_string());
            if use_stdin_prompt {
                args.push("--input-format".to_string());
                args.push("text".to_string());
            }

            args.push("--output-format".to_string());
            args.push("stream-json".to_string());
            args.push("--verbose".to_string());

            if let Some(m) = &model {
                if !m.trim().is_empty() {
                    args.push("--model".to_string());
                    args.push(m.clone());
                }
            }

            // Resume: -r <id>
            if let Some(resume_id) = resume_id.as_deref() {
                if !resume_id.trim().is_empty() {
                    args.push("-r".to_string());
                    args.push(resume_id.to_string());
                }
            }
            if !raw_prompt.is_empty() && !use_stdin_prompt {
                args.push(escape_shell_arg(&raw_prompt));
            }
            // if let Some(dir) = &project_id {
            //     args.push("--add-dir".to_string());
            //     args.push(dir.clone());
            //     envs.insert("WORKSPACE_DIR".to_string(), dir.clone());
            // }
        } else if cmd_type.contains("gemini") {
            // Matches examples like:
            // gemini "..." -y -o stream-json
            if use_stdin_prompt {
                args.push("-p".to_string());
                args.push(String::new());
            } else if !raw_prompt.is_empty() {
                args.push(escape_shell_arg(&raw_prompt));
            }

            args.push("-y".to_string());
            args.push("-o".to_string());
            args.push("stream-json".to_string());

            // Resume: -r <id> (e.g. -r latest)
            if let Some(resume_id) = resume_id.as_deref() {
                if !resume_id.trim().is_empty() {
                    args.push("-r".to_string());
                    args.push(resume_id.to_string());
                }
            }

            // Leave -y (YOLO) and auth concerns to the user's environment.
            if let Some(m) = &model {
                if !m.trim().is_empty() {
                    args.push("--m".to_string());
                    args.push(m.clone());
                }
            }

            // if let Some(dir) = &project_id {
            //     args.push("--include-directories".to_string());
            //     args.push(dir.clone());
            //     envs.insert("WORKSPACE_DIR".to_string(), dir.clone());
            // }
        } else {
            // Generic passthrough-ish fallback (previous behavior).
            if let Some(m) = model {
                args.push("--model".to_string());
                args.push(m);
            }
            if !raw_prompt.is_empty() {
                args.push(escape_shell_arg(&raw_prompt));
            }
        }

        Ok(core_api::BackendPlan {
            runner: Box::new(CodeCliRunnerPlugin::new()),
            session_args: core_api::RunnerStartArgs {
                cmd: exe_path,
                args,
                envs,
                cwd,
                stdin_payload,
            },
        })
    }
}

/// 解析可执行文件的完整路径
///
/// 优先级：
/// 1. 如果是绝对路径且存在，直接使用
/// 2. 从 npm 全局工具目录查找（支持 nvm/nvm-windows）
/// 3. 在系统 PATH 中查找
/// 4. 失败时返回错误
fn resolve_executable_path(backend: &str) -> Result<String> {
    use std::path::Path;

    let backend_path = Path::new(backend);

    // 1. 如果是绝对路径且存在，直接使用
    if backend_path.is_absolute() && backend_path.exists() {
        tracing::debug!("Using absolute path: {}", backend);
        return Ok(backend.to_string());
    }

    // 2. 提取命令名（去掉扩展名）
    let cmd_name = backend_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(backend);

    // 3. 从 npm 全局工具中查找
    match find_in_npm_global(cmd_name) {
        Ok(path) => {
            tracing::info!("Found in npm global: {} -> {}", cmd_name, path);
            return Ok(path);
        }
        Err(e) => {
            tracing::debug!("npm global search failed: {}", e);
        }
    }

    // 4. 在系统 PATH 中查找
    match find_in_system_path(cmd_name) {
        Some(path) => {
            tracing::info!("Found in system PATH: {} -> {}", cmd_name, path);
            return Ok(path);
        }
        None => {
            tracing::debug!("Not found in system PATH: {}", cmd_name);
        }
    }

    // 5. 都找不到时返回错误
    Err(anyhow::anyhow!(
        "Executable '{}' not found. Please ensure it's installed (e.g., npm install -g {}) \
        or provide the full path.",
        cmd_name,
        cmd_name
    ))
}

/// 提取命令类型（用于判断参数格式）
fn extract_command_type(backend: &str) -> String {
    use std::path::Path;

    Path::new(backend)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(backend)
        .to_lowercase()
}

/// 在 npm 全局目录中查找命令（仅二进制可执行文件）
fn find_in_npm_global(cmd: &str) -> Result<String> {
    // 获取 npm 全局 bin 目录
    let npm_bin = get_npm_global_bin()?;

    #[cfg(target_os = "windows")]
    {
        // Windows: 只查找 .exe 二进制文件
        let mut path = npm_bin.clone();
        path.push(format!("{}.cmd", cmd));
        if path.is_file() {
            return Ok(path.to_string_lossy().to_string());
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        // Unix: 查找没有扩展名的可执行文件
        let mut path = npm_bin.clone();
        path.push(cmd);
        if path.is_file() && is_executable(&path) {
            return Ok(path.to_string_lossy().to_string());
        }
    }

    Err(anyhow::anyhow!(
        "Binary executable '{}' not found in npm global",
        cmd
    ))
}

/// 在系统 PATH 中查找命令（仅二进制可执行文件）
fn find_in_system_path(cmd: &str) -> Option<String> {
    use std::env;

    let path_env = env::var_os("PATH")?;

    for dir in env::split_paths(&path_env) {
        #[cfg(target_os = "windows")]
        {
            // Windows: 只查找 .exe 文件
            let candidate = dir.join(format!("{}.exe", cmd));
            if candidate.is_file() {
                return Some(candidate.to_string_lossy().to_string());
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            // Unix: 查找没有扩展名的可执行文件
            let candidate = dir.join(cmd);
            if candidate.is_file() && is_executable(&candidate) {
                return Some(candidate.to_string_lossy().to_string());
            }
        }
    }

    None
}

/// 检查文件是否为可执行文件（Unix 平台需要检查权限）
#[cfg(not(target_os = "windows"))]
fn is_executable(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    if let Ok(metadata) = std::fs::metadata(path) {
        let permissions = metadata.permissions();
        // 检查是否有任何执行权限位（用户/组/其他）
        permissions.mode() & 0o111 != 0
    } else {
        false
    }
}

/// 获取 npm 全局 bin 目录
fn get_npm_global_bin() -> Result<std::path::PathBuf> {
    use std::env;
    use std::process::Command;

    // 策略1: 检查 NVM 环境变量（nvm-windows 和 nvm 都支持）
    // NVM_BIN 指向当前激活版本的 bin 目录
    if let Ok(nvm_bin) = env::var("NVM_BIN") {
        let path = std::path::PathBuf::from(&nvm_bin);
        if path.is_dir() {
            tracing::info!("Using NVM_BIN: {}", path.display());
            return Ok(path);
        } else {
            tracing::debug!("NVM_BIN exists but not a directory: {}", nvm_bin);
        }
    }

    // 策略2: 检查 NVM_SYMLINK (nvm-windows)
    #[cfg(target_os = "windows")]
    if let Ok(nvm_symlink) = env::var("NVM_SYMLINK") {
        let path = std::path::PathBuf::from(&nvm_symlink);
        if path.is_dir() {
            tracing::info!("Using NVM_SYMLINK: {}", path.display());
            return Ok(path);
        }
    }

    // 策略3: 调用 npm bin -g
    let output = Command::new("npm")
        .args(["bin", "-g"])
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to execute 'npm bin -g': {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(anyhow::anyhow!(
            "npm bin -g failed with exit code {:?}\nstdout: {}\nstderr: {}",
            output.status.code(),
            stdout.trim(),
            stderr.trim()
        ));
    }

    let path_str = String::from_utf8(output.stdout)
        .map_err(|e| anyhow::anyhow!("Invalid UTF-8 in npm output: {}", e))?
        .trim()
        .to_string();

    if path_str.is_empty() {
        return Err(anyhow::anyhow!("npm bin -g returned empty output"));
    }

    let path = std::path::PathBuf::from(&path_str);
    if !path.exists() {
        return Err(anyhow::anyhow!(
            "npm global bin directory does not exist: {}",
            path_str
        ));
    }

    if !path.is_dir() {
        return Err(anyhow::anyhow!(
            "npm global bin path is not a directory: {}",
            path_str
        ));
    }

    tracing::info!("npm global bin directory: {}", path.display());
    Ok(path)
}

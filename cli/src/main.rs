//! CLI 二进制入口：解析命令行参数、加载配置、初始化 tracing，并把控制权交给 `app`/`commands`。
use clap::Parser;
use core_api::{AppContext, CliError, RunnerError};
use memex_cli::commands::cli;
use memex_core::api as core_api;
use memex_plugins::services::PluginServicesFactory;
use std::sync::Arc;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

/// 检查是否应该使用远程模式
fn should_use_remote_mode(ctx: &AppContext) -> bool {
    ctx.cfg().http_server.mode == "remote"
}

/// 确保服务器正在运行（如果未运行则自动启动）
async fn ensure_server_running(server_url: &str) -> Result<(), CliError> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()
        .map_err(|e| CliError::Io(std::io::Error::other(e)))?;

    let health_url = format!("{}/health", server_url);

    // 检查服务器是否运行
    match client.get(&health_url).send().await {
        Ok(resp) if resp.status().is_success() => return Ok(()),
        _ => {} // 服务器未运行，继续启动
    }

    Err(CliError::Command(
        "Server failed to start within timeout".to_string(),
    ))
}

static LOG_GUARD: std::sync::OnceLock<tracing_appender::non_blocking::WorkerGuard> =
    std::sync::OnceLock::new();

/// 设置 Windows 控制台为 UTF-8 编码
#[cfg(windows)]
fn enable_utf8_console() {
    use std::ffi::c_uint;
    const CP_UTF8: c_uint = 65001;

    #[link(name = "kernel32")]
    extern "system" {
        fn SetConsoleOutputCP(wCodePageID: c_uint) -> i32;
        fn SetConsoleCP(wCodePageID: c_uint) -> i32;
    }

    unsafe {
        SetConsoleOutputCP(CP_UTF8);
        SetConsoleCP(CP_UTF8);
    }
}

#[cfg(not(windows))]
fn enable_utf8_console() {
    // Unix 系统默认使用 UTF-8，无需设置
}

#[tokio::main]
async fn main() {
    enable_utf8_console();

    let exit = match real_main().await {
        Ok(code) => code,
        Err(e) => {
            eprintln!("{e}");
            exit_code_for_error(&e)
        }
    };

    std::process::exit(exit);
}

async fn real_main() -> Result<i32, CliError> {
    let mut args = cli::Args::parse();
    let cfg = core_api::load_default().map_err(|e| CliError::Config(e.to_string()))?;
    init_tracing(&cfg.logging).map_err(CliError::Command)?;

    let services_factory: Option<Arc<dyn core_api::ServicesFactory>> =
        Some(Arc::new(PluginServicesFactory));
    let ctx = AppContext::new(cfg, services_factory)
        .await
        .map_err(CliError::Runner)?;

    let cmd = args.command.take();

    if let Some(cmd) = cmd {
        return dispatch(cmd, args, ctx).await;
    }
    Ok(0)
}

fn exit_code_for_error(e: &CliError) -> i32 {
    // 0: success
    // 11: config error
    // 20: runner start / IO error
    // 40: policy deny (usually returned as a normal exit code, not as an error)
    // 50: internal/uncategorized
    match e {
        CliError::Config(_) => 11,
        CliError::Runner(re) => match re {
            RunnerError::Config(_) => 11,
            RunnerError::Spawn(_) => 20,
            RunnerError::StreamIo { .. } => 20,
            RunnerError::Plugin(_) => 50,
            RunnerError::Stdio(_) => 50,
        },
        CliError::Io(_) => 20,
        CliError::Command(_) => 20,
        CliError::Replay(_) => 50,
        CliError::Anyhow(_) => 50,
    }
}

async fn dispatch(cmd: cli::Commands, args: cli::Args, ctx: AppContext) -> Result<i32, CliError> {
    let is_remote = should_use_remote_mode(&ctx);
    let server_url = format!(
        "http://{}:{}",
        ctx.cfg().http_server.host,
        ctx.cfg().http_server.port
    );
    match cmd {
        cli::Commands::Run(run_args) => {
            if is_remote {
                // 远程模式：确保服务器运行，然后通过 HTTP 调用 Core Server
                ensure_server_running(&server_url).await?;
            }
            let exit =
                memex_cli::app::run_app_with_config(args, Some(run_args), None, &is_remote, &ctx)
                    .await?;
            Ok(exit)
        }
        cli::Commands::Replay(replay_args) => {
            let core_args = core_api::ReplayArgs {
                events: replay_args.events,
                run_id: replay_args.run_id,
                format: replay_args.format,
                set: replay_args.set,
                rerun_gatekeeper: replay_args.rerun_gatekeeper,
            };
            core_api::replay_cmd(core_args).map_err(CliError::Replay)?;
            Ok(0)
        }
        cli::Commands::Resume(resume_args) => {
            if is_remote {
                // 远程模式：确保服务器运行，然后通过 HTTP 调用 Core Server
                ensure_server_running(&server_url).await?;
            }
            let recover_id = Some(resume_args.run_id.clone());
            let exit: i32 = memex_cli::app::run_app_with_config(
                args,
                Some(resume_args.run_args),
                recover_id,
                &is_remote,
                &ctx,
            )
            .await?;
            Ok(exit)
        }
        cli::Commands::Search(search_args) => {
            memex_cli::commands::memory::handle_search(search_args, &ctx).await?;
            Ok(0)
        }
        cli::Commands::RecordCandidate(record_args) => {
            memex_cli::commands::memory::handle_record_candidate(record_args, &ctx).await?;
            Ok(0)
        }
        cli::Commands::RecordHit(hit_args) => {
            memex_cli::commands::memory::handle_record_hit(hit_args, &ctx).await?;
            Ok(0)
        }
        cli::Commands::RecordValidation(validation_args) => {
            memex_cli::commands::memory::handle_record_validation(validation_args, &ctx).await?;
            Ok(0)
        }
        cli::Commands::RecordSession(session_args) => {
            memex_cli::commands::memory::handle_record_session(session_args, &ctx).await?;
            Ok(0)
        }
        cli::Commands::HttpServer(http_args) => {
            memex_cli::http::server::handle_http_server(http_args, &ctx).await?;
            Ok(0)
        }
        cli::Commands::Init(init_args) => {
            memex_cli::commands::init::handle_init(init_args, &ctx).await?;
            Ok(0)
        }
        cli::Commands::Sync(sync_args) => {
            memex_cli::commands::sync::handle_sync(sync_args, &ctx).await?;
            Ok(0)
        }
        cli::Commands::Db(db_args) => {
            memex_cli::commands::db::handle_db(db_args, &ctx).await?;
            Ok(0)
        }
    }
}

fn init_tracing(logging: &core_api::LoggingConfig) -> Result<(), String> {
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
        let file_name = "memex-cli.log";
        // let appender = tracing_appender::rolling::daily(dir, file_name);
        let file_appender = RollingFileAppender::builder()
            .rotation(Rotation::DAILY)
            .max_log_files(3)
            .filename_prefix(file_name)
            .build(dir)
            .unwrap();
        let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
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

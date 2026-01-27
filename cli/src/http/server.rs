//! HTTP服务器生命周期管理

use super::{
    middleware::{create_middleware_stack, request_logger},
    routes::create_router,
    AppState,
};
use crate::commands::cli::HttpServerArgs;
use axum::middleware;
use memex_core::api::{AppContext, CliError};
use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;
use tokio::signal;
use tokio::sync::broadcast;
use tracing::{info, warn};
use uuid::Uuid;

/// HTTP服务器配置
#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".into(),
            port: 8080,
        }
    }
}

/// 获取服务器状态文件目录
fn get_servers_dir() -> Result<PathBuf, CliError> {
    let home = dirs::home_dir()
        .ok_or_else(|| CliError::Command("Cannot find home directory".to_string()))?;
    let servers_dir = home.join(".memex").join("servers");
    fs::create_dir_all(&servers_dir)
        .map_err(|e| CliError::Command(format!("Failed to create servers directory: {e}")))?;
    Ok(servers_dir)
}

/// 写入服务器状态文件
fn write_state_file(session_id: &str, port: u16, host: &str) -> Result<(), CliError> {
    let servers_dir = get_servers_dir()?;
    let state_file = servers_dir.join("memex.state");

    let state = serde_json::json!({
        "session_id": session_id,
        "port": port,
        "pid": std::process::id(),
        "url": format!("http://{}:{}", host, port),
        "started_at": chrono::Local::now().to_rfc3339()
    });

    fs::write(&state_file, serde_json::to_string_pretty(&state).unwrap())
        .map_err(|e| CliError::Command(format!("Failed to write state file: {e}")))?;

    tracing::info!("State file written to: {}", state_file.display());
    Ok(())
}

/// 处理 http-server 命令
pub async fn handle_http_server(args: HttpServerArgs, ctx: &AppContext) -> Result<(), CliError> {
    // 使用用户提供的 session_id 或生成新的
    let session_id = args
        .session_id
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    // 合并配置：CLI 参数优先，配置文件作为默认值
    let config = &ctx.cfg().http_server;

    // 如果 CLI 参数与默认值相同，则使用配置文件中的值
    let port = if args.port == 8080 {
        config.port
    } else {
        args.port
    };

    let host = if args.host == "127.0.0.1" {
        config.host.clone()
    } else {
        args.host.clone()
    };

    // 构建 Services
    let services = ctx
        .build_services(ctx.cfg())
        .await
        .map_err(CliError::Runner)?;

    // 创建 shutdown channel
    let (shutdown_tx, _) = broadcast::channel(1);

    // 创建 AppState（传入完整配置）
    let state = AppState::new(
        session_id.clone(),
        ctx.clone(),
        services,
        ctx.cfg().clone(),
        shutdown_tx,
    );

    // 写入状态文件（在服务器启动前）
    write_state_file(&session_id, port, &host)?;

    // 启动服务器
    tracing::info!(
        "Starting HTTP server on {}:{} (session: {})",
        host,
        port,
        session_id
    );

    start_server(session_id, host, port, state)
        .await
        .map_err(|e: Box<dyn std::error::Error + Send + Sync>| CliError::Command(e.to_string()))?;

    Ok(())
}

/// 启动HTTP服务器
pub async fn start_server(
    session_id: String,
    host: String,
    port: u16,
    state: AppState,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let config = ServerConfig { host, port };

    start_server_with_config(session_id, config, state).await
}

/// 使用自定义配置启动HTTP服务器
pub async fn start_server_with_config(
    session_id: String,
    config: ServerConfig,
    state: AppState,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    info!(
        "Starting HTTP server on {}:{} (session: {})",
        config.host, config.port, session_id
    );

    // 构建路由
    let router = create_router(state.clone());

    // 添加中间件
    let app = router
        .layer(middleware::from_fn(request_logger))
        .layer(create_middleware_stack());

    // 解析地址
    let addr: SocketAddr = format!("{}:{}", config.host, config.port).parse()?;

    // 创建服务器
    info!("HTTP server listening on http://{}", addr);

    // 启动服务器
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    // 克隆 shutdown_rx 用于优雅关闭
    let mut shutdown_rx = state.shutdown_tx.subscribe();

    // 启动服务器并等待关闭信号
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            // 等待关闭信号
            tokio::select! {
                _ = signal::ctrl_c() => {
                    info!("Received Ctrl+C signal");
                }
                _ = shutdown_rx.recv() => {
                    info!("Received shutdown signal from API");
                }
                _ = wait_for_sigterm() => {
                    info!("Received SIGTERM signal");
                }
            }

            info!("Starting graceful shutdown...");
        })
        .await?;

    info!("Server shutdown complete");

    // 删除状态文件
    let servers_dir = get_servers_dir()?;
    let state_file_path = servers_dir.join("memex.state");
    if let Err(e) = fs::remove_file(&state_file_path) {
        warn!("Failed to remove state file: {}", e);
    } else {
        info!("State file removed: {}", state_file_path.display());
    }

    Ok(())
}

/// 等待 SIGTERM 信号（Unix系统）
#[cfg(unix)]
async fn wait_for_sigterm() {
    use tokio::signal::unix::{signal, SignalKind};

    let mut sigterm = signal(SignalKind::terminate()).expect("Failed to setup SIGTERM handler");
    sigterm.recv().await;
}

/// Windows 系统不支持 SIGTERM，使用空操作
#[cfg(not(unix))]
async fn wait_for_sigterm() {
    // Windows不支持SIGTERM，永久等待（实际上会被 Ctrl+C 或 shutdown API 中断）
    std::future::pending::<()>().await
}

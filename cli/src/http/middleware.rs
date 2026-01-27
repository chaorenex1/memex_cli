//! HTTP中间件配置

use axum::{
    body::Body,
    http::{header, HeaderValue, Method, Request},
    middleware::Next,
    response::Response,
};
use std::time::{Duration, Instant};
use tower_http::{cors::CorsLayer, timeout::TimeoutLayer, trace::TraceLayer};
use tracing::{info, warn};

/// 创建中间件栈
pub fn create_middleware_stack() -> tower::layer::util::Stack<CorsLayer, TimeoutLayer> {
    tower::layer::util::Stack::new(create_cors_layer(), create_timeout_layer())
}

/// 创建CORS中间件 - 仅允许localhost
fn create_cors_layer() -> CorsLayer {
    // 使用函数来验证 origin 是否为 localhost
    CorsLayer::new()
        .allow_origin(tower_http::cors::AllowOrigin::predicate(
            |origin: &HeaderValue, _| {
                origin
                    .to_str()
                    .map(|s| {
                        s.starts_with("http://localhost")
                            || s.starts_with("https://localhost")
                            || s.starts_with("http://127.0.0.1")
                            || s.starts_with("https://127.0.0.1")
                    })
                    .unwrap_or(false)
            },
        ))
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION, header::ACCEPT])
        .allow_credentials(true)
        .max_age(Duration::from_secs(3600))
}

/// 创建超时中间件 - 30秒
fn create_timeout_layer() -> TimeoutLayer {
    TimeoutLayer::new(Duration::from_secs(30))
}

/// 创建请求日志layer（用于HTTP请求追踪）
pub fn create_trace_layer(
) -> TraceLayer<tower_http::classify::SharedClassifier<tower_http::classify::ServerErrorsAsFailures>>
{
    TraceLayer::new_for_http()
}

/// 请求日志中间件（手动实现，用于记录详细信息）
pub async fn request_logger(req: Request<Body>, next: Next) -> Response {
    let method = req.method().clone();
    let uri = req.uri().clone();
    let start = Instant::now();

    // 执行请求
    let response = next.run(req).await;

    let duration = start.elapsed();
    let status = response.status();

    // 根据状态码选择日志级别
    if status.is_success() {
        info!(
            method = %method,
            uri = %uri,
            status = %status.as_u16(),
            duration_ms = %duration.as_millis(),
            "Request completed"
        );
    } else if status.is_client_error() || status.is_server_error() {
        warn!(
            method = %method,
            uri = %uri,
            status = %status.as_u16(),
            duration_ms = %duration.as_millis(),
            "Request failed"
        );
    } else {
        info!(
            method = %method,
            uri = %uri,
            status = %status.as_u16(),
            duration_ms = %duration.as_millis(),
            "Request completed"
        );
    }

    response
}

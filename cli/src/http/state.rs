//! HTTP服务器状态管理

use chrono::{DateTime, Local};
use memex_core::api::{AppConfig, AppContext, Services};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tokio::sync::broadcast;

/// 应用状态（在所有handlers间共享）
#[derive(Clone)]
pub struct AppState {
    pub session_id: String,
    pub ctx: Arc<AppContext>,
    pub services: Arc<Services>,
    pub config: Arc<AppConfig>,
    pub stats: Arc<RwLock<ServerStats>>,
    pub shutdown_tx: broadcast::Sender<()>,
}

impl AppState {
    pub fn new(
        session_id: String,
        ctx: AppContext,
        services: Services,
        config: AppConfig,
        shutdown_tx: broadcast::Sender<()>,
    ) -> Self {
        Self {
            session_id,
            ctx: Arc::new(ctx),
            services: Arc::new(services),
            config: Arc::new(config),
            stats: Arc::new(RwLock::new(ServerStats::new())),
            shutdown_tx,
        }
    }
}

/// 服务器统计信息
pub struct ServerStats {
    pub requests_total: u64,
    pub requests_by_endpoint: HashMap<String, u64>,
    pub errors_total: u64,
    pub start_time: DateTime<Local>,
}

impl ServerStats {
    pub fn new() -> Self {
        Self {
            requests_total: 0,
            requests_by_endpoint: HashMap::new(),
            errors_total: 0,
            start_time: Local::now(),
        }
    }

    pub fn increment_request(&mut self, endpoint: &str) {
        self.requests_total += 1;
        *self
            .requests_by_endpoint
            .entry(endpoint.to_string())
            .or_insert(0) += 1;
    }

    pub fn increment_error(&mut self) {
        self.errors_total += 1;
    }

    pub fn uptime_seconds(&self) -> f64 {
        let now = Local::now();
        (now - self.start_time).num_milliseconds() as f64 / 1000.0
    }
}

impl Default for ServerStats {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_stats_new() {
        let stats = ServerStats::new();
        assert_eq!(stats.requests_total, 0);
        assert_eq!(stats.errors_total, 0);
        assert!(stats.uptime_seconds() < 1.0);
    }

    #[test]
    fn test_increment_request() {
        let mut stats = ServerStats::new();
        stats.increment_request("/api/v1/search");
        stats.increment_request("/api/v1/search");
        stats.increment_request("/health");

        assert_eq!(stats.requests_total, 3);
        assert_eq!(
            *stats.requests_by_endpoint.get("/api/v1/search").unwrap(),
            2
        );
        assert_eq!(*stats.requests_by_endpoint.get("/health").unwrap(), 1);
    }

    #[test]
    fn test_increment_error() {
        let mut stats = ServerStats::new();
        stats.increment_error();
        stats.increment_error();
        assert_eq!(stats.errors_total, 2);
    }
}

//! 状态类型定义

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 应用级状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppState {
    /// 应用启动时间
    pub started_at: DateTime<Utc>,
    /// 当前活跃会话数
    pub active_sessions: usize,
    /// 已完成会话数
    pub completed_sessions: usize,
    /// 全局配置版本
    pub config_version: String,
    /// 是否处于维护模式
    pub maintenance_mode: bool,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            started_at: Utc::now(),
            active_sessions: 0,
            completed_sessions: 0,
            config_version: "1.0.0".to_string(),
            maintenance_mode: false,
        }
    }
}

/// 运行时状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeState {
    /// 当前运行 ID
    pub run_id: Option<String>,
    /// Runner 进程 PID
    pub runner_pid: Option<u32>,
    /// 当前阶段
    pub phase: RuntimePhase,
    /// 处理的工具事件数量
    pub tool_events_count: usize,
    /// 记忆检索命中数
    pub memory_hits: usize,
    /// Gatekeeper 决策
    pub gatekeeper_decision: Option<GatekeeperDecisionSnapshot>,
    /// 性能指标
    pub metrics: RuntimeMetrics,
}

impl Default for RuntimeState {
    fn default() -> Self {
        Self {
            run_id: None,
            runner_pid: None,
            phase: RuntimePhase::Idle,
            tool_events_count: 0,
            memory_hits: 0,
            gatekeeper_decision: None,
            metrics: RuntimeMetrics::default(),
        }
    }
}

/// 运行时阶段
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuntimePhase {
    /// 空闲
    Idle,
    /// 初始化
    Initializing,
    /// 记忆检索中
    MemorySearch,
    /// Runner 启动中
    RunnerStarting,
    /// Runner 运行中
    RunnerRunning,
    /// 工具事件处理中
    ProcessingToolEvents,
    /// Gatekeeper 评估中
    GatekeeperEvaluating,
    /// 记忆沉淀中
    MemoryPersisting,
    /// 完成
    Completed,
    /// 失败
    Failed,
}

/// Gatekeeper 决策快照
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatekeeperDecisionSnapshot {
    pub should_write_candidate: bool,
    pub reasons: Vec<String>,
    pub signals: HashMap<String, serde_json::Value>,
}

/// 运行时性能指标
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RuntimeMetrics {
    /// 启动耗时（毫秒）
    pub startup_duration_ms: Option<u64>,
    /// 记忆检索耗时（毫秒）
    pub memory_search_duration_ms: Option<u64>,
    /// Runner 执行耗时（毫秒）
    pub runner_duration_ms: Option<u64>,
    /// 总耗时（毫秒）
    pub total_duration_ms: Option<u64>,
    /// 处理速率（事件/秒）
    pub events_per_second: Option<f64>,
}

/// 状态事件
#[derive(Debug, Clone, Serialize)]
pub enum StateEvent {
    /// 应用启动
    AppStarted { timestamp: DateTime<Utc> },
    /// 会话创建
    SessionCreated {
        session_id: String,
        timestamp: DateTime<Utc>,
    },
    /// 会话状态变更
    SessionStateChanged {
        session_id: String,
        old_phase: RuntimePhase,
        new_phase: RuntimePhase,
        timestamp: DateTime<Utc>,
    },
    /// 工具事件收到
    ToolEventReceived {
        session_id: String,
        event_count: usize,
        timestamp: DateTime<Utc>,
    },
    /// 记忆命中
    MemoryHit {
        session_id: String,
        hit_count: usize,
        timestamp: DateTime<Utc>,
    },
    /// Gatekeeper 决策
    GatekeeperDecision {
        session_id: String,
        should_write: bool,
        timestamp: DateTime<Utc>,
    },
    /// 会话完成
    SessionCompleted {
        session_id: String,
        exit_code: i32,
        duration_ms: u64,
        timestamp: DateTime<Utc>,
    },
    /// 会话失败
    SessionFailed {
        session_id: String,
        error: String,
        timestamp: DateTime<Utc>,
    },
    /// 应用关闭
    AppShutdown { timestamp: DateTime<Utc> },
}

impl StateEvent {
    pub fn timestamp(&self) -> DateTime<Utc> {
        match self {
            Self::AppStarted { timestamp } => *timestamp,
            Self::SessionCreated { timestamp, .. } => *timestamp,
            Self::SessionStateChanged { timestamp, .. } => *timestamp,
            Self::ToolEventReceived { timestamp, .. } => *timestamp,
            Self::MemoryHit { timestamp, .. } => *timestamp,
            Self::GatekeeperDecision { timestamp, .. } => *timestamp,
            Self::SessionCompleted { timestamp, .. } => *timestamp,
            Self::SessionFailed { timestamp, .. } => *timestamp,
            Self::AppShutdown { timestamp } => *timestamp,
        }
    }

    pub fn session_id(&self) -> Option<&str> {
        match self {
            Self::SessionCreated { session_id, .. }
            | Self::SessionStateChanged { session_id, .. }
            | Self::ToolEventReceived { session_id, .. }
            | Self::MemoryHit { session_id, .. }
            | Self::GatekeeperDecision { session_id, .. }
            | Self::SessionCompleted { session_id, .. }
            | Self::SessionFailed { session_id, .. } => Some(session_id),
            _ => None,
        }
    }
}

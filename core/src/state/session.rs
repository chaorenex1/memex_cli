//! 会话状态管理

use super::types::{RuntimePhase, RuntimeState};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// 会话状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    /// 会话唯一 ID
    pub session_id: String,
    /// 运行 ID（可能是恢复的）
    pub run_id: Option<String>,
    /// 会话状态
    pub status: SessionStatus,
    /// 运行时状态
    pub runtime: RuntimeState,
    /// 创建时间
    pub created_at: DateTime<Utc>,
    /// 更新时间
    pub updated_at: DateTime<Utc>,
    /// 完成时间
    pub completed_at: Option<DateTime<Utc>>,
    /// 附加元数据
    pub metadata: HashMap<String, String>,
}

impl SessionState {
    /// 创建新会话
    pub fn new(run_id: Option<String>) -> Self {
        let now = Utc::now();
        Self {
            session_id: Uuid::new_v4().to_string(),
            run_id: run_id.clone(),
            status: SessionStatus::Created,
            runtime: RuntimeState {
                run_id,
                ..Default::default()
            },
            created_at: now,
            updated_at: now,
            completed_at: None,
            metadata: HashMap::new(),
        }
    }

    /// 转换到新阶段
    pub fn transition_to(&mut self, new_phase: RuntimePhase) {
        self.runtime.phase = new_phase;
        self.updated_at = Utc::now();

        // 更新会话状态
        self.status = match new_phase {
            RuntimePhase::Idle | RuntimePhase::Initializing => SessionStatus::Created,
            RuntimePhase::MemorySearch
            | RuntimePhase::RunnerStarting
            | RuntimePhase::RunnerRunning
            | RuntimePhase::ProcessingToolEvents
            | RuntimePhase::GatekeeperEvaluating
            | RuntimePhase::MemoryPersisting => SessionStatus::Running,
            RuntimePhase::Completed => SessionStatus::Completed,
            RuntimePhase::Failed => SessionStatus::Failed,
        };

        // 如果完成或失败，设置完成时间
        if matches!(
            self.status,
            SessionStatus::Completed | SessionStatus::Failed
        ) {
            self.completed_at = Some(Utc::now());
        }
    }

    /// 增加工具事件计数
    pub fn increment_tool_events(&mut self, count: usize) {
        self.runtime.tool_events_count += count;
        self.updated_at = Utc::now();
    }

    /// 增加记忆命中计数
    pub fn increment_memory_hits(&mut self, count: usize) {
        self.runtime.memory_hits += count;
        self.updated_at = Utc::now();
    }

    /// 设置 Runner PID
    pub fn set_runner_pid(&mut self, pid: u32) {
        self.runtime.runner_pid = Some(pid);
        self.updated_at = Utc::now();
    }

    /// 设置 Gatekeeper 决策
    pub fn set_gatekeeper_decision(&mut self, decision: super::types::GatekeeperDecisionSnapshot) {
        self.runtime.gatekeeper_decision = Some(decision);
        self.updated_at = Utc::now();
    }

    /// 更新性能指标
    pub fn update_metrics<F>(&mut self, f: F)
    where
        F: FnOnce(&mut super::types::RuntimeMetrics),
    {
        f(&mut self.runtime.metrics);
        self.updated_at = Utc::now();
    }

    /// 设置元数据
    pub fn set_metadata(&mut self, key: String, value: String) {
        self.metadata.insert(key, value);
        self.updated_at = Utc::now();
    }

    /// 获取会话持续时间（毫秒）
    pub fn duration_ms(&self) -> u64 {
        let end_time = self.completed_at.unwrap_or_else(Utc::now);
        (end_time - self.created_at).num_milliseconds() as u64
    }

    /// 是否活跃
    pub fn is_active(&self) -> bool {
        matches!(self.status, SessionStatus::Running)
    }

    /// 是否已完成
    pub fn is_completed(&self) -> bool {
        matches!(
            self.status,
            SessionStatus::Completed | SessionStatus::Failed | SessionStatus::Cancelled
        )
    }
}

/// 会话状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionStatus {
    /// 已创建
    Created,
    /// 运行中
    Running,
    /// 已完成
    Completed,
    /// 失败
    Failed,
    /// 已取消
    Cancelled,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_creation() {
        let session = SessionState::new(Some("test-run-id".to_string()));
        assert_eq!(session.status, SessionStatus::Created);
        assert_eq!(session.runtime.phase, RuntimePhase::Idle);
        assert_eq!(session.run_id, Some("test-run-id".to_string()));
    }

    #[test]
    fn test_session_transition() {
        let mut session = SessionState::new(None);

        session.transition_to(RuntimePhase::RunnerRunning);
        assert_eq!(session.status, SessionStatus::Running);
        assert_eq!(session.runtime.phase, RuntimePhase::RunnerRunning);

        session.transition_to(RuntimePhase::Completed);
        assert_eq!(session.status, SessionStatus::Completed);
        assert!(session.completed_at.is_some());
    }

    #[test]
    fn test_tool_events_increment() {
        let mut session = SessionState::new(None);
        session.increment_tool_events(5);
        assert_eq!(session.runtime.tool_events_count, 5);
        session.increment_tool_events(3);
        assert_eq!(session.runtime.tool_events_count, 8);
    }
}

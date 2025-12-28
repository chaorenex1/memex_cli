//! 状态管理器

use super::session::{SessionState, SessionStatus};
use super::types::{AppState, RuntimePhase, StateEvent};
use anyhow::{Context, Result};
use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

/// 状态管理器
#[derive(Clone)]
pub struct StateManager {
    inner: Arc<StateManagerInner>,
}

struct StateManagerInner {
    /// 应用状态
    app_state: RwLock<AppState>,
    /// 所有会话
    sessions: RwLock<HashMap<String, SessionState>>,
    /// 事件广播通道
    event_tx: broadcast::Sender<StateEvent>,
}

impl StateManager {
    /// 创建新的状态管理器
    pub fn new() -> Self {
        let (event_tx, _) = broadcast::channel(1000);

        let inner = StateManagerInner {
            app_state: RwLock::new(AppState::default()),
            sessions: RwLock::new(HashMap::new()),
            event_tx,
        };

        Self {
            inner: Arc::new(inner),
        }
    }

    /// 获取状态管理器句柄
    pub fn handle(&self) -> StateManagerHandle {
        StateManagerHandle {
            manager: self.clone(),
        }
    }

    /// 订阅状态事件
    pub fn subscribe(&self) -> broadcast::Receiver<StateEvent> {
        self.inner.event_tx.subscribe()
    }

    /// 发送状态事件
    async fn emit_event(&self, event: StateEvent) {
        let _ = self.inner.event_tx.send(event);
    }

    /// 发送工具事件统计
    pub async fn emit_tool_event_received(&self, session_id: &str, event_count: usize) {
        self.emit_event(StateEvent::ToolEventReceived {
            session_id: session_id.to_string(),
            event_count,
            timestamp: Utc::now(),
        })
        .await;
    }

    /// 发送记忆命中统计
    pub async fn emit_memory_hit(&self, session_id: &str, hit_count: usize) {
        self.emit_event(StateEvent::MemoryHit {
            session_id: session_id.to_string(),
            hit_count,
            timestamp: Utc::now(),
        })
        .await;
    }

    /// 发送 Gatekeeper 决策
    pub async fn emit_gatekeeper_decision(&self, session_id: &str, should_write: bool) {
        self.emit_event(StateEvent::GatekeeperDecision {
            session_id: session_id.to_string(),
            should_write,
            timestamp: Utc::now(),
        })
        .await;
    }

    /// 获取应用状态
    pub async fn get_app_state(&self) -> AppState {
        self.inner.app_state.read().await.clone()
    }

    /// 更新应用状态
    pub async fn update_app_state<F>(&self, f: F) -> Result<()>
    where
        F: FnOnce(&mut AppState),
    {
        let mut state = self.inner.app_state.write().await;
        f(&mut state);
        Ok(())
    }

    /// 创建新会话
    pub async fn create_session(&self, run_id: Option<String>) -> Result<String> {
        let session = SessionState::new(run_id);
        let session_id = session.session_id.clone();

        // 更新应用状态
        {
            let mut app_state = self.inner.app_state.write().await;
            app_state.active_sessions += 1;
        }

        // 存储会话
        {
            let mut sessions = self.inner.sessions.write().await;
            sessions.insert(session_id.clone(), session);
        }

        // 发送事件
        self.emit_event(StateEvent::SessionCreated {
            session_id: session_id.clone(),
            timestamp: Utc::now(),
        })
        .await;

        Ok(session_id)
    }

    /// 获取会话状态
    pub async fn get_session(&self, session_id: &str) -> Result<SessionState> {
        let sessions = self.inner.sessions.read().await;
        sessions
            .get(session_id)
            .cloned()
            .context("Session not found")
    }

    /// 更新会话状态
    pub async fn update_session<F>(&self, session_id: &str, f: F) -> Result<()>
    where
        F: FnOnce(&mut SessionState),
    {
        let mut sessions = self.inner.sessions.write().await;
        let session = sessions.get_mut(session_id).context("Session not found")?;
        f(session);
        Ok(())
    }

    /// 转换会话阶段
    pub async fn transition_session_phase(
        &self,
        session_id: &str,
        new_phase: RuntimePhase,
    ) -> Result<()> {
        let old_phase = {
            let sessions = self.inner.sessions.read().await;
            sessions
                .get(session_id)
                .map(|s| s.runtime.phase)
                .context("Session not found")?
        };

        self.update_session(session_id, |session| {
            session.transition_to(new_phase);
        })
        .await?;

        self.emit_event(StateEvent::SessionStateChanged {
            session_id: session_id.to_string(),
            old_phase,
            new_phase,
            timestamp: Utc::now(),
        })
        .await;

        Ok(())
    }

    /// 完成会话
    pub async fn complete_session(&self, session_id: &str, exit_code: i32) -> Result<()> {
        let duration_ms = {
            self.update_session(session_id, |session| {
                session.transition_to(RuntimePhase::Completed);
            })
            .await?;

            let session = self.get_session(session_id).await?;
            session.duration_ms()
        };

        // 更新应用状态
        {
            let mut app_state = self.inner.app_state.write().await;
            app_state.active_sessions = app_state.active_sessions.saturating_sub(1);
            app_state.completed_sessions += 1;
        }

        self.emit_event(StateEvent::SessionCompleted {
            session_id: session_id.to_string(),
            exit_code,
            duration_ms,
            timestamp: Utc::now(),
        })
        .await;

        Ok(())
    }

    /// 会话失败
    pub async fn fail_session(&self, session_id: &str, error: String) -> Result<()> {
        self.update_session(session_id, |session| {
            session.transition_to(RuntimePhase::Failed);
        })
        .await?;

        // 更新应用状态
        {
            let mut app_state = self.inner.app_state.write().await;
            app_state.active_sessions = app_state.active_sessions.saturating_sub(1);
        }

        self.emit_event(StateEvent::SessionFailed {
            session_id: session_id.to_string(),
            error,
            timestamp: Utc::now(),
        })
        .await;

        Ok(())
    }

    /// 获取所有活跃会话
    pub async fn get_active_sessions(&self) -> Vec<SessionState> {
        let sessions = self.inner.sessions.read().await;
        sessions
            .values()
            .filter(|s| s.is_active())
            .cloned()
            .collect()
    }

    /// 获取会话统计
    pub async fn get_session_stats(&self) -> SessionStats {
        let sessions = self.inner.sessions.read().await;
        let mut stats = SessionStats::default();

        for session in sessions.values() {
            match session.status {
                SessionStatus::Created => stats.created += 1,
                SessionStatus::Running => stats.running += 1,
                SessionStatus::Completed => stats.completed += 1,
                SessionStatus::Failed => stats.failed += 1,
                SessionStatus::Cancelled => stats.cancelled += 1,
            }
        }

        stats
    }

    /// 清理已完成的会话（可选保留最近 N 个）
    pub async fn cleanup_completed_sessions(&self, keep_recent: usize) -> Result<usize> {
        let mut sessions = self.inner.sessions.write().await;

        let mut completed: Vec<_> = sessions
            .iter()
            .filter(|(_, s)| s.is_completed())
            .map(|(id, s)| (id.clone(), s.completed_at.unwrap()))
            .collect();

        completed.sort_by(|a, b| b.1.cmp(&a.1));

        let to_remove: Vec<_> = completed
            .iter()
            .skip(keep_recent)
            .map(|(id, _)| id.clone())
            .collect();

        let count = to_remove.len();
        for id in to_remove {
            sessions.remove(&id);
        }

        Ok(count)
    }
}

impl Default for StateManager {
    fn default() -> Self {
        Self::new()
    }
}

/// 状态管理器句柄
#[derive(Clone)]
pub struct StateManagerHandle {
    manager: StateManager,
}

impl StateManagerHandle {
    /// 获取内部管理器
    pub fn inner(&self) -> &StateManager {
        &self.manager
    }

    /// 创建会话
    pub async fn create_session(&self, run_id: Option<String>) -> Result<String> {
        self.manager.create_session(run_id).await
    }

    /// 转换阶段
    pub async fn transition_phase(&self, session_id: &str, phase: RuntimePhase) -> Result<()> {
        self.manager
            .transition_session_phase(session_id, phase)
            .await
    }

    /// 完成会话
    pub async fn complete(&self, session_id: &str, exit_code: i32) -> Result<()> {
        self.manager.complete_session(session_id, exit_code).await
    }

    /// 失败会话
    pub async fn fail(&self, session_id: &str, error: String) -> Result<()> {
        self.manager.fail_session(session_id, error).await
    }
}

/// 会话统计
#[derive(Debug, Default, Clone)]
pub struct SessionStats {
    pub created: usize,
    pub running: usize,
    pub completed: usize,
    pub failed: usize,
    pub cancelled: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_state_manager_creation() {
        let manager = StateManager::new();
        let app_state = manager.get_app_state().await;
        assert_eq!(app_state.active_sessions, 0);
        assert_eq!(app_state.completed_sessions, 0);
    }

    #[tokio::test]
    async fn test_session_lifecycle() {
        let manager = StateManager::new();

        // 创建会话
        let session_id = manager
            .create_session(Some("test-run".to_string()))
            .await
            .unwrap();
        assert_eq!(manager.get_app_state().await.active_sessions, 1);

        // 转换阶段
        manager
            .transition_session_phase(&session_id, RuntimePhase::RunnerRunning)
            .await
            .unwrap();
        let session = manager.get_session(&session_id).await.unwrap();
        assert_eq!(session.runtime.phase, RuntimePhase::RunnerRunning);

        // 完成会话
        manager.complete_session(&session_id, 0).await.unwrap();
        assert_eq!(manager.get_app_state().await.active_sessions, 0);
        assert_eq!(manager.get_app_state().await.completed_sessions, 1);
    }

    #[tokio::test]
    async fn test_event_subscription() {
        let manager = StateManager::new();
        let mut rx = manager.subscribe();

        let session_id = manager.create_session(None).await.unwrap();

        match rx.recv().await {
            Ok(StateEvent::SessionCreated { session_id: id, .. }) => {
                assert_eq!(id, session_id);
            }
            _ => panic!("Expected SessionCreated event"),
        }
    }
}

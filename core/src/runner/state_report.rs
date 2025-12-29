use std::sync::Arc;

use crate::state::types::RuntimePhase;
use crate::state::StateManager;

pub struct StateReporter {
    manager: Option<Arc<StateManager>>,
    session_id: Option<String>,
    tool_events_started: bool,
}

impl StateReporter {
    pub fn new(manager: Option<Arc<StateManager>>, session_id: Option<String>) -> Self {
        Self {
            manager,
            session_id,
            tool_events_started: false,
        }
    }

    pub fn on_tool_event(&mut self) {
        let (Some(manager), Some(session_id)) = (&self.manager, &self.session_id) else {
            return;
        };

        if !self.tool_events_started {
            self.tool_events_started = true;
            let manager = manager.clone();
            let session_id = session_id.clone();
            tokio::spawn(async move {
                let _ = manager
                    .transition_session_phase(&session_id, RuntimePhase::ProcessingToolEvents)
                    .await;
            });
        }

        let manager = manager.clone();
        let session_id = session_id.clone();
        tokio::spawn(async move {
            let _ = manager
                .update_session(&session_id, |session| {
                    session.increment_tool_events(1);
                })
                .await;
            manager.emit_tool_event_received(&session_id, 1).await;
        });
    }

    pub async fn set_runner_duration_ms(&self, duration_ms: u64) {
        let (Some(manager), Some(session_id)) = (&self.manager, &self.session_id) else {
            return;
        };

        let _ = manager
            .update_session(session_id, |session| {
                session.update_metrics(|metrics| {
                    metrics.runner_duration_ms = Some(duration_ms);
                });
            })
            .await;
    }
}


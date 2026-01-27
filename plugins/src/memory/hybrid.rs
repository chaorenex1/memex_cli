//! Hybrid memory plugin with local storage and remote synchronization.
//!
//! Combines LocalMemoryPlugin with SyncService for automatic
//! bidirectional synchronization.

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value as JsonValue};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

use memex_core::api::{
    MemoryPlugin, QACandidatePayload, QAHitsPayload, QASearchPayload, QAValidationPayload,
    SearchMatch, SyncStatusReport, SyncStrategy, SyncableMemory, TaskGradeResult,
};

use super::local::{LocalMemoryConfig, LocalMemoryPlugin};
use super::sync::{
    ConflictRecord, HttpRemoteMemoryClient, RemoteMemoryClient, SyncConfig, SyncEvent, SyncService,
};

/// Configuration for the hybrid memory plugin.
pub struct HybridMemoryConfig {
    pub local: LocalMemoryConfig,
    pub remote_base_url: String,
    pub remote_api_key: String,
    pub remote_timeout_ms: u64,
    pub sync_strategy: SyncStrategy,
    pub sync: SyncConfig,
}

/// Hybrid memory plugin with automatic synchronization.
pub struct HybridMemoryPlugin {
    local: Arc<LocalMemoryPlugin>,
    sync: Option<Arc<Mutex<SyncService>>>,
    sync_tx: Option<mpsc::UnboundedSender<SyncEvent>>,
}

impl HybridMemoryPlugin {
    /// Create a new hybrid memory plugin.
    pub async fn new(config: HybridMemoryConfig) -> Result<Self> {
        // Create local plugin first
        let local = Arc::new(Self::create_local_plugin(config.local.clone()).await?);

        // Create sync service if enabled
        let (sync, sync_tx) = if config.sync.enabled {
            // Use the new store() method from LocalMemoryPlugin
            let store = local.store();

            let remote: Arc<dyn RemoteMemoryClient> = Arc::new(HttpRemoteMemoryClient::new(
                config.remote_base_url,
                config.remote_api_key,
                config.remote_timeout_ms,
            ));

            let sync_service = Arc::new(Mutex::new(SyncService::new(store, remote, config.sync)));

            // Create event channel and start sync service
            let (tx, rx) = mpsc::unbounded_channel();
            let sync_clone = Arc::clone(&sync_service);

            // Spawn sync service in background
            tokio::spawn(async move {
                SyncService::run(sync_clone, rx).await;
            });

            // Trigger initial sync
            let _ = tx.send(SyncEvent::SyncNow);

            (Some(sync_service), Some(tx))
        } else {
            (None, None)
        };

        Ok(Self {
            local,
            sync,
            sync_tx,
        })
    }

    async fn create_local_plugin(config: LocalMemoryConfig) -> Result<LocalMemoryPlugin> {
        LocalMemoryPlugin::new(config).await
    }

    /// Trigger a sync operation.
    pub fn trigger_sync(&self) {
        if let Some(ref tx) = self.sync_tx {
            let _ = tx.send(SyncEvent::SyncNow);
        }
    }

    /// Get current sync status.
    pub fn sync_status(&self) -> Option<super::sync::SyncStatusReport> {
        self.sync
            .as_ref()
            .and_then(|s| s.try_lock().ok().map(|svc| svc.status()))
    }

    /// Get pending conflicts from the sync service.
    pub async fn get_conflicts(&self) -> Result<Vec<ConflictRecord>> {
        if let Some(ref sync) = self.sync {
            let svc = sync.lock().await;
            Ok(svc.get_conflicts().to_vec())
        } else {
            Ok(vec![])
        }
    }

    /// Get reference to the local plugin.
    pub fn local(&self) -> &Arc<LocalMemoryPlugin> {
        &self.local
    }

    /// Check if sync is enabled.
    pub fn is_sync_enabled(&self) -> bool {
        self.sync.is_some()
    }
}

/// Convert local SyncStatusReport to core SyncStatusReport.
fn to_core_status_report(local: super::sync::SyncStatusReport) -> SyncStatusReport {
    SyncStatusReport {
        is_syncing: local.sync_in_progress,
        last_sync_at: local.last_sync_at.map(|dt| dt.to_rfc3339()),
        next_sync_at: None, // Could be calculated from interval
        pending_count: local.pending_upload,
        conflict_count: local.pending_conflicts,
        state: if local.sync_in_progress {
            "syncing".to_string()
        } else if local.is_online {
            "idle".to_string()
        } else {
            "offline".to_string()
        },
    }
}

/// Implement the core SyncableMemory trait for HybridMemoryPlugin.
impl SyncableMemory for HybridMemoryPlugin {
    fn trigger_sync(&self) -> bool {
        if self.sync_tx.is_some() {
            self.trigger_sync();
            true
        } else {
            false
        }
    }

    fn sync_status(&self) -> Option<SyncStatusReport> {
        self.sync_status().map(to_core_status_report)
    }

    async fn get_conflicts(&self) -> Result<JsonValue, anyhow::Error> {
        let conflicts = self.get_conflicts().await?;
        Ok(json!({
            "count": conflicts.len(),
            "conflicts": conflicts.iter().enumerate().map(|(i, c)| {
                json!({
                    "id": i,
                    "local_id": c.local_id,
                    "remote_id": c.remote_id,
                    "local_updated_at": c.local_updated_at.to_rfc3339(),
                    "remote_updated_at": c.remote_updated_at.to_rfc3339(),
                    "local_question": c.local_question,
                    "remote_question": c.remote_question,
                })
            }).collect::<Vec<_>>()
        }))
    }

    fn is_sync_enabled(&self) -> bool {
        self.is_sync_enabled()
    }
}

#[async_trait]
impl MemoryPlugin for HybridMemoryPlugin {
    fn name(&self) -> &str {
        "hybrid-memory"
    }

    async fn search(&self, payload: QASearchPayload) -> Result<Vec<SearchMatch>> {
        // Search locally (LocalFirst strategy)
        self.local.search(payload).await
    }

    async fn record_hit(&self, payload: QAHitsPayload) -> Result<()> {
        // Record locally and let sync handle remote
        self.local.record_hit(payload).await
    }

    async fn record_candidate(&self, payload: QACandidatePayload) -> Result<()> {
        // Record locally and let sync handle remote
        self.local.record_candidate(payload).await
    }

    async fn record_validation(&self, payload: QAValidationPayload) -> Result<()> {
        // Record locally and let sync handle remote
        self.local.record_validation(payload).await
    }

    async fn task_grade(&self, prompt: String) -> Result<TaskGradeResult> {
        // Delegate to local plugin
        self.local.task_grade(prompt).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_strategy_display() {
        // Just verify the types compile
        let _ = SyncStrategy::LocalFirst;
        let _ = SyncStrategy::RemoteFirst;
    }
}

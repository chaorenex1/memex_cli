//! Optional trait for memory plugins that support synchronization.

/// Status report for sync operations.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SyncStatusReport {
    /// Whether sync is currently running
    pub is_syncing: bool,
    /// Last sync timestamp (RFC3339)
    pub last_sync_at: Option<String>,
    /// Next sync scheduled timestamp (RFC3339)
    pub next_sync_at: Option<String>,
    /// Number of pending items to sync
    pub pending_count: usize,
    /// Number of conflicts
    pub conflict_count: usize,
    /// Current sync state description
    pub state: String,
}

/// Trait for memory plugins that support synchronization.
///
/// This trait can be used with downcasting to access sync-specific
/// functionality from a `MemoryPlugin` trait object.
pub trait SyncableMemory: Send + Sync {
    /// Trigger a sync operation.
    ///
    /// Returns `true` if sync was triggered, `false` if sync is not available.
    fn trigger_sync(&self) -> bool;

    /// Get current sync status.
    fn sync_status(&self) -> Option<SyncStatusReport>;

    /// Get pending conflicts.
    ///
    /// Returns a JSON value containing conflict information.
    fn get_conflicts(
        &self,
    ) -> impl futures::Future<Output = Result<serde_json::Value, anyhow::Error>>;

    /// Check if this plugin supports sync operations.
    fn is_sync_enabled(&self) -> bool;
}

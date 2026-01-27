//! Synchronization service for local and remote memory.
//!
//! Handles bidirectional synchronization between LanceDB local storage
//! and remote HTTP API.

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Mutex};
use tokio::time::{interval, sleep};

use super::conflict::{
    resolve_conflict, ConflictRecord, ConflictResolution, ConflictResolutionResult,
};
use crate::memory::lance::{LanceStore, QAItem, SyncStatus};
use crate::memory::sync::remote_client::{RemoteMemoryClient, UploadResult};

/// Configuration for the sync service.
#[derive(Debug, Clone)]
pub struct SyncConfig {
    /// Enable automatic background sync
    pub enabled: bool,

    /// Auto-sync interval
    pub interval: Duration,

    /// Batch size for upload
    pub batch_size: usize,

    /// Maximum retry attempts
    pub max_retries: usize,

    /// Retry delay multiplier (exponential backoff)
    pub retry_delay_ms: u64,

    /// Conflict resolution strategy
    pub conflict_resolution: ConflictResolution,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval: Duration::from_secs(300), // 5 minutes
            batch_size: 50,
            max_retries: 3,
            retry_delay_ms: 1000,
            conflict_resolution: ConflictResolution::LastWriteWins,
        }
    }
}

/// Events that can be sent to the sync service.
#[derive(Debug, Clone)]
pub enum SyncEvent {
    /// Trigger immediate sync
    SyncNow,
    /// An item was modified locally
    ItemChanged(String),
    /// Network status changed
    Online(bool),
    /// Get sync status
    GetStatus,
}

/// Sync status report.
#[derive(Debug, Clone)]
pub struct SyncStatusReport {
    pub last_sync_at: Option<chrono::DateTime<chrono::Utc>>,
    pub pending_upload: usize,
    pub pending_conflicts: usize,
    pub is_online: bool,
    pub sync_in_progress: bool,
}

/// Synchronization service.
pub struct SyncService {
    store: Arc<LanceStore>,
    remote: Arc<dyn RemoteMemoryClient>,
    config: SyncConfig,
    is_online: bool,
    last_sync_at: Option<chrono::DateTime<chrono::Utc>>,
    pending_upload: usize,
    pending_conflicts: Vec<ConflictRecord>,
    sync_in_progress: bool,
}

impl SyncService {
    /// Create a new sync service.
    pub fn new(
        store: Arc<LanceStore>,
        remote: Arc<dyn RemoteMemoryClient>,
        config: SyncConfig,
    ) -> Self {
        Self {
            store,
            remote,
            config,
            is_online: true, // Assume online initially
            last_sync_at: None,
            pending_upload: 0,
            pending_conflicts: vec![],
            sync_in_progress: false,
        }
    }

    /// Get current sync status.
    pub fn status(&self) -> SyncStatusReport {
        SyncStatusReport {
            last_sync_at: self.last_sync_at,
            pending_upload: self.pending_upload,
            pending_conflicts: self.pending_conflicts.len(),
            is_online: self.is_online,
            sync_in_progress: self.sync_in_progress,
        }
    }

    /// Run the sync service with an event channel.
    pub async fn run(sync: Arc<Mutex<Self>>, mut rx: mpsc::UnboundedReceiver<SyncEvent>) {
        let interval_duration = {
            let svc = sync.lock().await;
            svc.config.interval
        };
        let mut ticker = interval(interval_duration);

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    let mut svc = sync.lock().await;
                    if svc.config.enabled && svc.is_online {
                        if let Err(e) = svc.do_sync().await {
                            tracing::error!("Auto-sync failed: {}", e);
                        }
                    }
                }
                Some(event) = rx.recv() => {
                    let mut svc = sync.lock().await;
                    match event {
                        SyncEvent::SyncNow => {
                            if let Err(e) = svc.do_sync().await {
                                tracing::error!("Manual sync failed: {}", e);
                            }
                        }
                        SyncEvent::Online(is_online) => {
                            svc.is_online = is_online;
                            if is_online {
                                tracing::info!("Network online, starting sync");
                                if let Err(e) = svc.do_sync().await {
                                    tracing::error!("Sync after online failed: {}", e);
                                }
                            } else {
                                tracing::warn!("Network offline, sync paused");
                            }
                        }
                        SyncEvent::GetStatus => {
                            // Status is queried via status() method
                            // This could be enhanced to send status via a channel
                        }
                        SyncEvent::ItemChanged(id) => {
                            tracing::debug!("Item changed: {}", id);
                            // Mark for next sync cycle
                        }
                    }
                }
            }
        }
    }

    /// Perform a full sync cycle.
    async fn do_sync(&mut self) -> Result<(), anyhow::Error> {
        if self.sync_in_progress {
            tracing::warn!("Sync already in progress, skipping");
            return Ok(());
        }

        self.sync_in_progress = true;
        let start = std::time::Instant::now();

        // Refresh pending count at the start of sync cycle
        self.refresh_pending_count().await;

        tracing::info!(
            "Starting sync cycle ({} items pending)",
            self.pending_upload
        );

        // Step 1: Upload pending local changes
        self.upload_pending().await?;

        // Step 2: Download remote updates
        self.download_updates().await?;

        // Step 3: Handle conflicts
        self.resolve_conflicts().await?;

        // Step 4: Upload auxiliary records (validations, hits)
        self.upload_auxiliary().await?;

        // Refresh pending count after sync cycle
        self.refresh_pending_count().await;

        self.last_sync_at = Some(chrono::Utc::now());
        self.sync_in_progress = false;

        tracing::info!("Sync cycle completed in {:?}", start.elapsed());

        Ok(())
    }

    /// Refresh the pending upload count from the store.
    async fn refresh_pending_count(&mut self) {
        self.pending_upload = self.store.count_pending_sync().await.unwrap_or(0);
    }

    /// Upload pending local changes to remote.
    async fn upload_pending(&mut self) -> Result<(), anyhow::Error> {
        let pending = self.store.get_pending_sync().await?;

        if pending.is_empty() {
            tracing::debug!("No pending items to upload");
            return Ok(());
        }

        tracing::info!("Uploading {} pending items", pending.len());

        for batch in pending.chunks(self.config.batch_size) {
            let result = self.upload_with_retry(batch.to_vec()).await?;

            // Mark successfully uploaded items as synced
            let ids: Vec<String> = result
                .id_mapping
                .iter()
                .map(|(local_id, _)| local_id.clone())
                .collect();
            let remote_ids: Vec<String> = result
                .id_mapping
                .iter()
                .map(|(_, remote_id)| remote_id.clone())
                .collect();

            if !ids.is_empty() {
                self.store.mark_synced(ids, remote_ids).await?;
            }

            // Handle failed uploads
            for failed_id in result.failed {
                tracing::warn!("Failed to upload item: {}", failed_id);
            }
        }

        Ok(())
    }

    /// Upload items with retry logic.
    async fn upload_with_retry(&self, items: Vec<QAItem>) -> Result<UploadResult, anyhow::Error> {
        let mut last_error = None;

        for attempt in 0..self.config.max_retries {
            match self.remote.upload_items(items.clone()).await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    last_error = Some(e);
                    if attempt < self.config.max_retries - 1 {
                        let delay = self.config.retry_delay_ms * 2_u64.pow(attempt as u32);
                        tracing::warn!(
                            "Upload failed (attempt {}/{}), retrying in {}ms",
                            attempt + 1,
                            self.config.max_retries,
                            delay
                        );
                        sleep(Duration::from_millis(delay)).await;
                    }
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("Upload failed after retries")))
    }

    /// Download remote updates since last sync.
    async fn download_updates(&mut self) -> Result<(), anyhow::Error> {
        let since = self
            .last_sync_at
            .unwrap_or_else(|| chrono::Utc::now() - chrono::Duration::days(7));

        tracing::info!("Downloading updates since {}", since.to_rfc3339());

        let remote_items = self.remote.download_updates(since).await?;

        if remote_items.is_empty() {
            tracing::debug!("No remote updates");
            return Ok(());
        }

        tracing::info!("Downloaded {} remote items", remote_items.len());

        // Process each remote item
        for remote_item in remote_items {
            self.process_remote_item(remote_item).await?;
        }

        Ok(())
    }

    /// Process a single remote item.
    async fn process_remote_item(
        &mut self,
        remote_item: crate::memory::sync::remote_client::RemoteQAItem,
    ) -> Result<(), anyhow::Error> {
        // Check if local version exists
        if let Some(local_item) = self.store.get_qa(&remote_item.id).await? {
            // Both versions exist - check for conflict
            if local_item.sync_status == SyncStatus::Pending {
                // Local has uncommitted changes - potential conflict
                let resolution = resolve_conflict(
                    local_item,
                    remote_item.clone(),
                    self.config.conflict_resolution,
                );

                match resolution {
                    ConflictResolutionResult::UseLocal(local) => {
                        // Re-upload local version to win
                        let result = self.remote.upload_items(vec![*local]).await?;
                        if let Some((_, remote_id)) = result.id_mapping.first() {
                            self.store
                                .mark_synced(vec![remote_item.id.clone()], vec![remote_id.clone()])
                                .await?;
                        }
                    }
                    ConflictResolutionResult::UseRemote(remote) => {
                        // Update local with remote version
                        let remote = *remote;
                        let remote_id = remote.id.clone();
                        let mut updated = self.remote_to_local(remote);
                        updated.mark_synced(Some(remote_id));
                        self.store.upsert_qa(updated).await?;
                    }
                    ConflictResolutionResult::Merged(merged) => {
                        // Upload merged version
                        let mut merged = *merged;
                        let result = self.remote.upload_items(vec![merged.clone()]).await?;
                        if let Some((_, remote_id)) = result.id_mapping.first() {
                            merged.mark_synced(Some(remote_id.clone()));
                            self.store.upsert_qa(merged).await?;
                        }
                    }
                    ConflictResolutionResult::Manual { local, remote } => {
                        // Record conflict for manual resolution
                        self.pending_conflicts
                            .push(ConflictRecord::from_items(local.as_ref(), remote.as_ref()));
                    }
                }
            } else if local_item.updated_at < remote_item.updated_at {
                // Remote is newer - update local
                let remote_id = remote_item.id.clone();
                let mut updated = self.remote_to_local(remote_item);
                updated.mark_synced(Some(remote_id));
                self.store.upsert_qa(updated).await?;
            }
        } else {
            // Only remote exists - download to local
            let remote_id = remote_item.id.clone();
            let mut new_item = self.remote_to_local(remote_item);
            new_item.mark_synced(Some(remote_id));
            self.store.upsert_qa(new_item).await?;
        }

        Ok(())
    }

    /// Resolve pending conflicts.
    async fn resolve_conflicts(&mut self) -> Result<(), anyhow::Error> {
        if self.pending_conflicts.is_empty() {
            return Ok(());
        }

        tracing::info!("Resolving {} conflicts", self.pending_conflicts.len());

        // For now, just log the conflicts
        // In the future, this could be resolved via CLI commands
        for conflict in &self.pending_conflicts {
            tracing::warn!(
                "Conflict: local({}:{:?}) vs remote({}:{:?})",
                conflict.local_id,
                conflict.local_updated_at,
                conflict.remote_id,
                conflict.remote_updated_at
            );
        }

        Ok(())
    }

    /// Upload auxiliary records (validations, hits).
    async fn upload_auxiliary(&self) -> Result<(), anyhow::Error> {
        // Get pending validation records
        let validations = self.store.get_pending_validations().await?;
        if !validations.is_empty() {
            tracing::info!("Uploading {} validation records", validations.len());
            let validation_ids: Vec<String> = validations.iter().map(|v| v.id.clone()).collect();
            if let Err(e) = self.remote.upload_validations(validations).await {
                tracing::warn!("Failed to upload validations: {}", e);
                // Continue with hits upload even if validations fail
            } else {
                // Mark as synced
                let _ = self.store.mark_validations_synced(validation_ids).await;
            }
        }

        // Get pending hit records
        let hits = self.store.get_pending_hits().await?;
        if !hits.is_empty() {
            tracing::info!("Uploading {} hit records", hits.len());
            let hit_ids: Vec<String> = hits.iter().map(|h| h.id.clone()).collect();
            if let Err(e) = self.remote.upload_hits(hits).await {
                tracing::warn!("Failed to upload hits: {}", e);
            } else {
                // Mark as synced
                let _ = self.store.mark_hits_synced(hit_ids).await;
            }
        }

        Ok(())
    }

    /// Convert a remote QA item to a local QA item.
    fn remote_to_local(&self, remote: crate::memory::sync::remote_client::RemoteQAItem) -> QAItem {
        QAItem {
            id: remote.id.clone(),
            project_id: remote.project_id,
            question: remote.question,
            answer: remote.answer,
            question_vector: None, // Will be vectorized on next search
            tags: remote.tags,
            confidence: remote.confidence,
            validation_level: remote.validation_level.into(),
            source: remote.source,
            author: remote.author,
            metadata: remote.metadata,
            created_at: remote.created_at,
            updated_at: remote.updated_at,
            synced_at: Some(chrono::Utc::now()),
            sync_status: SyncStatus::Synced,
            remote_id: Some(remote.id.clone()),
            is_vectorized: false,
        }
    }

    /// Get pending conflicts.
    pub fn get_conflicts(&self) -> &[ConflictRecord] {
        &self.pending_conflicts
    }

    /// Resolve a specific conflict by choosing local or remote.
    pub async fn resolve_conflict(
        &mut self,
        conflict_id: usize,
        choice: ConflictResolution,
    ) -> Result<(), anyhow::Error> {
        if conflict_id >= self.pending_conflicts.len() {
            return Err(anyhow::anyhow!("Invalid conflict ID"));
        }

        let conflict = self.pending_conflicts.remove(conflict_id);

        // Re-fetch items and resolve
        let local = self
            .store
            .get_qa(&conflict.local_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Local item not found"))?;

        let remote = crate::memory::sync::remote_client::RemoteQAItem {
            id: conflict.remote_id.clone(),
            project_id: local.project_id.clone(),
            question: conflict.remote_question.clone(),
            answer: conflict.remote_answer.clone(),
            tags: vec![],
            confidence: local.confidence,
            validation_level: u8::from(local.validation_level),
            source: None,
            author: None,
            metadata: serde_json::json!({}),
            created_at: conflict.local_updated_at,
            updated_at: conflict.remote_updated_at,
        };

        let resolution = resolve_conflict(local, remote, choice);

        match resolution {
            ConflictResolutionResult::UseLocal(local) => {
                self.store.upsert_qa(*local).await?;
            }
            ConflictResolutionResult::UseRemote(remote) => {
                let mut updated = self.remote_to_local(*remote);
                updated.mark_synced(Some(conflict.remote_id.clone()));
                self.store.upsert_qa(updated).await?;
            }
            ConflictResolutionResult::Merged(merged) => {
                self.store.upsert_qa(*merged).await?;
            }
            ConflictResolutionResult::Manual { .. } => {
                // Put back to pending
                self.pending_conflicts.push(conflict);
            }
        }

        Ok(())
    }
}

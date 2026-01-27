//! Conflict resolution strategies for synchronization.

use crate::memory::lance::{QAItem, SyncStatus};
use crate::memory::sync::remote_client::RemoteQAItem;
use chrono::DateTime;
use serde::Serialize;

// Use ConflictResolution from core_api
pub use memex_core::api::ConflictResolution;

/// Result of resolving a conflict.
#[derive(Debug, Clone)]
pub enum ConflictResolutionResult {
    /// Use the local version
    UseLocal(Box<QAItem>),
    /// Use the remote version
    UseRemote(Box<RemoteQAItem>),
    /// Merge both versions (with updated_at from winner)
    Merged(Box<QAItem>),
    /// Requires manual resolution
    Manual {
        local: Box<QAItem>,
        remote: Box<RemoteQAItem>,
    },
}

/// Resolve a conflict between local and remote versions of a QA item.
pub fn resolve_conflict(
    local: QAItem,
    remote: RemoteQAItem,
    strategy: ConflictResolution,
) -> ConflictResolutionResult {
    match strategy {
        ConflictResolution::LastWriteWins => {
            if local.updated_at > remote.updated_at {
                ConflictResolutionResult::UseLocal(Box::new(local))
            } else {
                ConflictResolutionResult::UseRemote(Box::new(remote))
            }
        }
        ConflictResolution::LocalWins => ConflictResolutionResult::UseLocal(Box::new(local)),
        ConflictResolution::RemoteWins => ConflictResolutionResult::UseRemote(Box::new(remote)),
        ConflictResolution::Manual => ConflictResolutionResult::Manual {
            local: Box::new(local),
            remote: Box::new(remote),
        },
    }
}

/// Merge remote item into local item.
///
/// This creates a new item that combines both versions:
/// - Takes the most recent updated_at timestamp
/// - Takes the most recent validation_level
/// - Merges tags from both versions
/// - Keeps local answer if local has higher validation_level
pub fn merge_items(local: QAItem, remote: RemoteQAItem) -> QAItem {
    // Merge tags
    let mut merged_tags = local.tags.clone();
    for tag in &remote.tags {
        if !merged_tags.contains(tag) {
            merged_tags.push(tag.clone());
        }
    }

    // Use the most recent validation level
    let merged_level = if u8::from(local.validation_level) >= remote.validation_level {
        local.validation_level
    } else {
        remote.validation_level.into()
    };

    // Update sync status and remote_id
    let mut merged = QAItem {
        id: local.id.clone(),
        project_id: local.project_id.clone(),
        question: local.question.clone(),
        answer: local.answer.clone(),
        question_vector: local.question_vector.clone(),
        tags: merged_tags,
        confidence: local.confidence.max(remote.confidence),
        validation_level: merged_level,
        source: local.source.clone().or(remote.source),
        author: local.author.clone().or(remote.author),
        metadata: if local.metadata.is_object() && !remote.metadata.is_object() {
            local.metadata
        } else {
            remote.metadata
        },
        created_at: local.created_at.min(remote.created_at),
        updated_at: local.updated_at.max(remote.updated_at),
        synced_at: Some(chrono::Utc::now()),
        sync_status: SyncStatus::Synced,
        remote_id: Some(remote.id.clone()),
        is_vectorized: local.is_vectorized,
    };

    merged.mark_synced(Some(remote.id));
    merged
}

/// Conflict record for manual resolution.
#[derive(Debug, Clone, Serialize)]
pub struct ConflictRecord {
    pub local_id: String,
    pub local_updated_at: DateTime<chrono::Utc>,
    pub local_question: String,
    pub local_answer: String,
    pub remote_id: String,
    pub remote_updated_at: DateTime<chrono::Utc>,
    pub remote_question: String,
    pub remote_answer: String,
}

impl ConflictRecord {
    /// Create from local and remote items.
    pub fn from_items(local: &QAItem, remote: &RemoteQAItem) -> Self {
        Self {
            local_id: local.id.clone(),
            local_updated_at: local.updated_at,
            local_question: local.question.clone(),
            local_answer: local.answer.clone(),
            remote_id: remote.id.clone(),
            remote_updated_at: remote.updated_at,
            remote_question: remote.question.clone(),
            remote_answer: remote.answer.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_conflict_resolution_from_str() {
        assert_eq!(
            "last_write_wins".parse::<ConflictResolution>().unwrap(),
            ConflictResolution::LastWriteWins
        );
        assert_eq!(
            "local_wins".parse::<ConflictResolution>().unwrap(),
            ConflictResolution::LocalWins
        );
        assert_eq!(
            "remote_wins".parse::<ConflictResolution>().unwrap(),
            ConflictResolution::RemoteWins
        );
        assert_eq!(
            "manual".parse::<ConflictResolution>().unwrap(),
            ConflictResolution::Manual
        );
    }

    #[test]
    fn test_last_write_wins_local() {
        let local = QAItem::new(
            "id1".to_string(),
            "proj1".to_string(),
            "local question".to_string(),
            "local answer".to_string(),
        );
        let remote = RemoteQAItem {
            id: "remote1".to_string(),
            project_id: "proj1".to_string(),
            question: "remote question".to_string(),
            answer: "remote answer".to_string(),
            tags: vec![],
            confidence: 0.5,
            validation_level: 0,
            source: None,
            author: None,
            metadata: serde_json::json!({}),
            created_at: Utc::now(),
            updated_at: Utc::now() - chrono::Duration::hours(1), // Older
        };

        let result = resolve_conflict(local, remote, ConflictResolution::LastWriteWins);

        match result {
            ConflictResolutionResult::UseLocal(_) => {}
            _ => panic!("Expected UseLocal"),
        }
    }

    #[test]
    fn test_last_write_wins_remote() {
        let mut local = QAItem::new(
            "id1".to_string(),
            "proj1".to_string(),
            "local question".to_string(),
            "local answer".to_string(),
        );
        local.updated_at = Utc::now() - chrono::Duration::hours(1); // Older

        let remote = RemoteQAItem {
            id: "remote1".to_string(),
            project_id: "proj1".to_string(),
            question: "remote question".to_string(),
            answer: "remote answer".to_string(),
            tags: vec![],
            confidence: 0.5,
            validation_level: 0,
            source: None,
            author: None,
            metadata: serde_json::json!({}),
            created_at: Utc::now(),
            updated_at: Utc::now(), // Newer
        };

        let result = resolve_conflict(local, remote, ConflictResolution::LastWriteWins);

        match result {
            ConflictResolutionResult::UseRemote(_) => {}
            _ => panic!("Expected UseRemote"),
        }
    }

    #[test]
    fn test_merge_items_tags() {
        let mut local = QAItem::new(
            "id1".to_string(),
            "proj1".to_string(),
            "question".to_string(),
            "answer".to_string(),
        );
        local.tags = vec!["tag1".to_string(), "tag2".to_string()];

        let remote = RemoteQAItem {
            id: "remote1".to_string(),
            project_id: "proj1".to_string(),
            question: "question".to_string(),
            answer: "answer".to_string(),
            tags: vec!["tag2".to_string(), "tag3".to_string()],
            confidence: 0.5,
            validation_level: 0,
            source: None,
            author: None,
            metadata: serde_json::json!({}),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let merged = merge_items(local, remote);

        // Should have all unique tags
        assert_eq!(merged.tags.len(), 3);
        assert!(merged.tags.contains(&"tag1".to_string()));
        assert!(merged.tags.contains(&"tag2".to_string()));
        assert!(merged.tags.contains(&"tag3".to_string()));
    }
}

//! Data models for LanceDB local memory storage.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Validation level for QA items.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[repr(u8)]
pub enum ValidationLevel {
    /// Newly extracted from execution (default)
    #[default]
    Candidate = 0,
    /// Validated through successful execution
    Verified = 1,
    /// Multiple successful validations
    Confirmed = 2,
    /// High-frequency use with consistent success
    GoldStandard = 3,
}

impl From<u8> for ValidationLevel {
    fn from(value: u8) -> Self {
        match value {
            0 => ValidationLevel::Candidate,
            1 => ValidationLevel::Verified,
            2 => ValidationLevel::Confirmed,
            _ => ValidationLevel::GoldStandard,
        }
    }
}

impl From<ValidationLevel> for u8 {
    fn from(level: ValidationLevel) -> Self {
        level as u8
    }
}

/// Synchronization status for a record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SyncStatus {
    /// Successfully synced with remote
    Synced,
    /// Pending upload to remote
    Pending,
    /// Conflict detected, requires resolution
    Conflict,
    /// Only exists locally (never been synced)
    #[default]
    LocalOnly,
    /// Only exists remotely (not fully downloaded)
    RemoteOnly,
}

impl std::fmt::Display for SyncStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SyncStatus::Synced => write!(f, "synced"),
            SyncStatus::Pending => write!(f, "pending"),
            SyncStatus::Conflict => write!(f, "conflict"),
            SyncStatus::LocalOnly => write!(f, "local_only"),
            SyncStatus::RemoteOnly => write!(f, "remote_only"),
        }
    }
}

impl std::str::FromStr for SyncStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "synced" => Ok(SyncStatus::Synced),
            "pending" => Ok(SyncStatus::Pending),
            "conflict" => Ok(SyncStatus::Conflict),
            "local_only" => Ok(SyncStatus::LocalOnly),
            "remote_only" => Ok(SyncStatus::RemoteOnly),
            _ => Err(format!("Invalid sync status: {}", s)),
        }
    }
}

/// QA item stored in LanceDB.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QAItem {
    pub id: String,
    pub project_id: String,
    pub question: String,
    pub answer: String,
    pub question_vector: Option<Vec<f32>>,
    pub tags: Vec<String>,
    pub confidence: f32,
    pub validation_level: ValidationLevel,
    pub source: Option<String>,
    pub author: Option<String>,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub synced_at: Option<DateTime<Utc>>,
    pub sync_status: SyncStatus,
    pub remote_id: Option<String>,
    pub is_vectorized: bool,
}

impl QAItem {
    /// Create a new QA item.
    pub fn new(id: String, project_id: String, question: String, answer: String) -> Self {
        let now = Utc::now();
        Self {
            id,
            project_id,
            question,
            answer,
            question_vector: None,
            tags: Vec::new(),
            confidence: 0.5,
            validation_level: ValidationLevel::Candidate,
            source: None,
            author: None,
            metadata: serde_json::json!({}),
            created_at: now,
            updated_at: now,
            synced_at: None,
            sync_status: SyncStatus::LocalOnly,
            remote_id: None,
            is_vectorized: false,
        }
    }

    /// Mark the item as modified and pending sync.
    pub fn mark_modified(&mut self) {
        self.updated_at = Utc::now();
        self.sync_status = SyncStatus::Pending;
    }

    /// Mark the item as synced.
    pub fn mark_synced(&mut self, remote_id: Option<String>) {
        self.synced_at = Some(Utc::now());
        self.sync_status = SyncStatus::Synced;
        if let Some(rid) = remote_id {
            self.remote_id = Some(rid);
        }
    }
}

/// Validation record for a QA item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationRecord {
    pub id: String,
    pub qa_id: String,
    pub result: ValidationResult,
    pub signal_strength: SignalStrength,
    pub success: Option<bool>,
    pub context: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub sync_status: SyncStatus,
}

/// Validation result from execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationResult {
    Pass,
    Fail,
    Unknown,
}

impl std::fmt::Display for ValidationResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationResult::Pass => write!(f, "pass"),
            ValidationResult::Fail => write!(f, "fail"),
            ValidationResult::Unknown => write!(f, "unknown"),
        }
    }
}

/// Signal strength for validation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalStrength {
    Strong,
    Weak,
}

impl std::fmt::Display for SignalStrength {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SignalStrength::Strong => write!(f, "strong"),
            SignalStrength::Weak => write!(f, "weak"),
        }
    }
}

/// Hit/click record for a QA item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HitRecord {
    pub id: String,
    pub qa_id: String,
    pub shown: bool,
    pub used: Option<bool>,
    pub session_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub sync_status: SyncStatus,
}

/// Sync operation for batch processing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SyncOperation {
    Upsert(QAItem),
    Delete(String), // qa_id
    Validate {
        qa_id: String,
        validation: ValidationRecord,
    },
    RecordHit {
        qa_id: String,
        hit: HitRecord,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validation_level_from_u8() {
        assert_eq!(ValidationLevel::from(0), ValidationLevel::Candidate);
        assert_eq!(ValidationLevel::from(1), ValidationLevel::Verified);
        assert_eq!(ValidationLevel::from(2), ValidationLevel::Confirmed);
        assert_eq!(ValidationLevel::from(3), ValidationLevel::GoldStandard);
        assert_eq!(ValidationLevel::from(99), ValidationLevel::GoldStandard);
    }

    #[test]
    fn test_sync_status_roundtrip() {
        let statuses = [
            SyncStatus::Synced,
            SyncStatus::Pending,
            SyncStatus::Conflict,
            SyncStatus::LocalOnly,
            SyncStatus::RemoteOnly,
        ];

        for status in statuses {
            let s = status.to_string();
            let parsed = s.parse::<SyncStatus>().unwrap();
            assert_eq!(status, parsed);
        }
    }

    #[test]
    fn test_qa_item_new() {
        let item = QAItem::new(
            "id1".to_string(),
            "proj1".to_string(),
            "question?".to_string(),
            "answer!".to_string(),
        );

        assert_eq!(item.id, "id1");
        assert_eq!(item.question_vector, None);
        assert_eq!(item.validation_level, ValidationLevel::Candidate);
        assert_eq!(item.sync_status, SyncStatus::LocalOnly);
        assert!(!item.is_vectorized);
    }

    #[test]
    fn test_qa_item_mark_modified() {
        let mut item = QAItem::new(
            "id1".to_string(),
            "proj1".to_string(),
            "question?".to_string(),
            "answer!".to_string(),
        );

        item.mark_synced(Some("remote1".to_string()));
        assert_eq!(item.sync_status, SyncStatus::Synced);

        item.mark_modified();
        assert_eq!(item.sync_status, SyncStatus::Pending);
    }
}

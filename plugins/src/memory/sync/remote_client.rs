//! Remote memory client for synchronization.
//!
//! Provides methods to interact with the remote memory API for
//! bidirectional synchronization.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::super::lance::{HitRecord, QAItem, ValidationRecord};

/// Trait for remote memory operations.
#[async_trait]
pub trait RemoteMemoryClient: Send + Sync {
    /// Upload a batch of QA items to remote.
    async fn upload_items(&self, items: Vec<QAItem>) -> Result<UploadResult, anyhow::Error>;

    /// Download updates since a given timestamp.
    async fn download_updates(
        &self,
        since: DateTime<Utc>,
    ) -> Result<Vec<RemoteQAItem>, anyhow::Error>;

    /// Upload validation records.
    async fn upload_validations(
        &self,
        validations: Vec<ValidationRecord>,
    ) -> Result<(), anyhow::Error>;

    /// Upload hit records.
    async fn upload_hits(&self, hits: Vec<HitRecord>) -> Result<(), anyhow::Error>;

    /// Check if remote is accessible.
    async fn health_check(&self) -> Result<bool, anyhow::Error>;
}

/// Result of uploading items to remote.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadResult {
    /// Mapping of local IDs to remote IDs
    pub id_mapping: Vec<(String, String)>,
    /// Items that failed to upload
    pub failed: Vec<String>,
}

/// QA item from remote server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteQAItem {
    pub id: String,
    pub project_id: String,
    pub question: String,
    pub answer: String,
    pub tags: Vec<String>,
    pub confidence: f32,
    pub validation_level: u8,
    pub source: Option<String>,
    pub author: Option<String>,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// HTTP-based remote memory client.
pub struct HttpRemoteMemoryClient {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
}

impl HttpRemoteMemoryClient {
    /// Create a new HTTP remote memory client.
    pub fn new(base_url: String, api_key: String, timeout_ms: u64) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(timeout_ms))
            .build()
            .unwrap();

        let base_url = base_url.trim_end_matches('/').to_string();

        Self {
            client,
            base_url,
            api_key,
        }
    }

    fn auth(&self, mut req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if !self.api_key.trim().is_empty() {
            req = req.bearer_auth(&self.api_key);
        }
        req
    }

    /// Build URL for a specific endpoint.
    fn url(&self, endpoint: &str) -> String {
        format!("{}/v1/qa/{}", self.base_url, endpoint)
    }
}

#[async_trait]
impl RemoteMemoryClient for HttpRemoteMemoryClient {
    async fn upload_items(&self, items: Vec<QAItem>) -> Result<UploadResult, anyhow::Error> {
        if items.is_empty() {
            return Ok(UploadResult {
                id_mapping: vec![],
                failed: vec![],
            });
        }

        let payload = UpsertPayload {
            items: items
                .into_iter()
                .map(|item| RemoteQAItem {
                    id: item.id.clone(),
                    project_id: item.project_id,
                    question: item.question,
                    answer: item.answer,
                    tags: item.tags,
                    confidence: item.confidence,
                    validation_level: u8::from(item.validation_level),
                    source: item.source,
                    author: item.author,
                    metadata: item.metadata,
                    created_at: item.created_at,
                    updated_at: item.updated_at,
                })
                .collect(),
        };

        let url = self.url("sync/upsert");
        let req = self.auth(self.client.post(&url).json(&payload));
        let resp = req.send().await?;

        if resp.status().is_success() {
            let result: UploadResult = resp.json().await?;
            Ok(result)
        } else {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            Err(anyhow::anyhow!("Upload failed: {} - {}", status, body))
        }
    }

    async fn download_updates(
        &self,
        since: DateTime<Utc>,
    ) -> Result<Vec<RemoteQAItem>, anyhow::Error> {
        let url = format!("{}?since={}", self.url("sync/updates"), since.to_rfc3339());
        let req = self.auth(self.client.get(&url));
        let resp = req.send().await?;

        if resp.status().is_success() {
            let items: Vec<RemoteQAItem> = resp.json().await?;
            Ok(items)
        } else {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            Err(anyhow::anyhow!("Download failed: {} - {}", status, body))
        }
    }

    async fn upload_validations(
        &self,
        validations: Vec<ValidationRecord>,
    ) -> Result<(), anyhow::Error> {
        if validations.is_empty() {
            return Ok(());
        }

        let payload = ValidationSyncPayload {
            validations: validations
                .into_iter()
                .map(|v| RemoteValidationRecord {
                    id: v.id,
                    qa_id: v.qa_id,
                    result: v.result.to_string(),
                    signal_strength: v.signal_strength.to_string(),
                    success: v.success,
                    context: v.context,
                    created_at: v.created_at,
                })
                .collect(),
        };

        let url = self.url("sync/validations");
        let req = self.auth(self.client.post(&url).json(&payload));
        let resp = req.send().await?;

        if resp.status().is_success() {
            Ok(())
        } else {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            Err(anyhow::anyhow!(
                "Validation sync failed: {} - {}",
                status,
                body
            ))
        }
    }

    async fn upload_hits(&self, hits: Vec<HitRecord>) -> Result<(), anyhow::Error> {
        if hits.is_empty() {
            return Ok(());
        }

        let payload = HitSyncPayload {
            hits: hits
                .into_iter()
                .map(|h| RemoteHitRecord {
                    id: h.id,
                    qa_id: h.qa_id,
                    shown: h.shown,
                    used: h.used,
                    session_id: h.session_id,
                    created_at: h.created_at,
                })
                .collect(),
        };

        let url = self.url("sync/hits");
        let req = self.auth(self.client.post(&url).json(&payload));
        let resp = req.send().await?;

        if resp.status().is_success() {
            Ok(())
        } else {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            Err(anyhow::anyhow!("Hit sync failed: {} - {}", status, body))
        }
    }

    async fn health_check(&self) -> Result<bool, anyhow::Error> {
        let url = self.url("health");
        let req = self.auth(self.client.get(&url));
        let resp = req.send().await?;

        Ok(resp.status().is_success())
    }
}

#[derive(Debug, Serialize)]
struct UpsertPayload {
    items: Vec<RemoteQAItem>,
}

#[derive(Debug, Serialize)]
struct ValidationSyncPayload {
    validations: Vec<RemoteValidationRecord>,
}

#[derive(Debug, Serialize)]
struct HitSyncPayload {
    hits: Vec<RemoteHitRecord>,
}

#[derive(Debug, Serialize)]
struct RemoteValidationRecord {
    id: String,
    qa_id: String,
    result: String,
    signal_strength: String,
    success: Option<bool>,
    context: serde_json::Value,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
struct RemoteHitRecord {
    id: String,
    qa_id: String,
    shown: bool,
    used: Option<bool>,
    session_id: Option<String>,
    created_at: DateTime<Utc>,
}

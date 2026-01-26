//! Local memory plugin using LanceDB for storage.
//!
//! Provides a MemoryPlugin implementation with local-first storage
//! using LanceDB for vector search and persistent storage.

use anyhow::{Context, Result};
use async_trait::async_trait;
use std::sync::Arc;
use uuid::Uuid;

use memex_core::api::{
    MemoryPlugin, QACandidatePayload, QAHitsPayload, QASearchPayload, QAValidationPayload,
    SearchMatch, TaskGradeResult,
};

use super::lance::{
    EmbeddingService, HitRecord, LanceStore, OllamaEmbeddingService, OpenAIEmbeddingService,
    QAItem, SignalStrength, SyncStatus, ValidationRecord, ValidationResult,
};

/// Configuration for the local memory plugin.
#[derive(Clone)]
pub struct LocalMemoryConfig {
    pub db_path: String,
    pub embedding: EmbeddingConfig,
    pub search_limit: u32,
    pub min_score: f32,
}

/// Embedding service configuration.
#[derive(Clone)]
pub enum EmbeddingConfig {
    Ollama {
        base_url: String,
        model: String,
        dimension: usize,
    },
    OpenAI {
        base_url: String,
        api_key: String,
        model: String,
    },
}

/// Local memory plugin using LanceDB.
pub struct LocalMemoryPlugin {
    store: Arc<LanceStore>,
    search_limit: u32,
    min_score: f32,
}

impl LocalMemoryPlugin {
    /// Create a new local memory plugin.
    pub async fn new(config: LocalMemoryConfig) -> Result<Self> {
        let embedding: Arc<dyn EmbeddingService> = match config.embedding {
            EmbeddingConfig::Ollama {
                base_url,
                model,
                dimension,
            } => Arc::new(OllamaEmbeddingService::new(base_url, model, dimension)),
            EmbeddingConfig::OpenAI {
                base_url,
                api_key,
                model,
            } => Arc::new(OpenAIEmbeddingService::new(base_url, api_key, model)),
        };

        let store = LanceStore::new(&config.db_path, embedding)
            .await
            .context("Failed to initialize LanceDB store")?;

        Ok(Self {
            store: Arc::new(store),
            search_limit: config.search_limit,
            min_score: config.min_score,
        })
    }

    /// Create with default configuration.
    pub async fn with_defaults(db_path: String) -> Result<Self> {
        Self::new(LocalMemoryConfig {
            db_path,
            embedding: EmbeddingConfig::Ollama {
                base_url: "http://localhost:11434".to_string(),
                model: "nomic-embed-text".to_string(),
                dimension: 768,
            },
            search_limit: 6,
            min_score: 0.2,
        })
        .await
    }

    /// Get a reference to the underlying LanceStore.
    /// This is used by HybridMemoryPlugin to access the store for sync operations.
    pub fn store(&self) -> Arc<LanceStore> {
        Arc::clone(&self.store)
    }
}

#[async_trait]
impl MemoryPlugin for LocalMemoryPlugin {
    fn name(&self) -> &str {
        "local-memory"
    }

    async fn search(&self, payload: QASearchPayload) -> Result<Vec<SearchMatch>> {
        let limit = if payload.limit == 0 {
            self.search_limit
        } else {
            payload.limit
        };
        let min_score = if payload.min_score <= 0.0 {
            self.min_score
        } else {
            payload.min_score
        };
        let results = self
            .store
            .search(
                &payload.project_id,
                &payload.query,
                limit as usize,
                min_score,
            )
            .await?;

        let matches = results
            .into_iter()
            .map(|(item, score)| SearchMatch {
                qa_id: item.id,
                project_id: Some(item.project_id),
                question: item.question,
                answer: item.answer,
                tags: item.tags,
                score,
                relevance: score,
                validation_level: item.validation_level as i32,
                level: None,
                trust: item.confidence,
                freshness: calculate_freshness(item.updated_at),
                confidence: item.confidence,
                status: "active".to_string(),
                summary: None,
                source: item.source,
                expiry_at: None,
                metadata: item.metadata,
            })
            .collect();

        Ok(matches)
    }

    async fn record_hit(&self, payload: QAHitsPayload) -> Result<()> {
        for reference in payload.references {
            let hit = HitRecord {
                id: Uuid::new_v4().to_string(),
                qa_id: reference.qa_id,
                shown: reference.shown.unwrap_or(true),
                used: reference.used,
                session_id: reference.message_id,
                created_at: chrono::Utc::now(),
                sync_status: SyncStatus::Pending,
            };
            self.store.add_hit(hit).await?;
        }
        Ok(())
    }

    async fn record_candidate(&self, payload: QACandidatePayload) -> Result<()> {
        let item = QAItem::new(
            Uuid::new_v4().to_string(),
            payload.project_id,
            payload.question,
            payload.answer,
        );

        // Set additional fields from payload
        let item_with_fields = QAItem {
            tags: payload.tags,
            confidence: payload.confidence,
            source: payload.source,
            metadata: payload.metadata,
            ..item
        };

        self.store.upsert_qa(item_with_fields).await?;
        Ok(())
    }

    async fn record_validation(&self, payload: QAValidationPayload) -> Result<()> {
        let result = payload.result.unwrap_or("unknown".to_string());
        let signal_strength = payload.signal_strength.unwrap_or("weak".to_string());

        let validation = ValidationRecord {
            id: Uuid::new_v4().to_string(),
            qa_id: payload.qa_id,
            result: match result.as_str() {
                "pass" => ValidationResult::Pass,
                "fail" => ValidationResult::Fail,
                _ => ValidationResult::Unknown,
            },
            signal_strength: match signal_strength.as_str() {
                "strong" => SignalStrength::Strong,
                _ => SignalStrength::Weak,
            },
            success: payload.success,
            context: payload.context.unwrap_or(serde_json::json!({})),
            created_at: chrono::Utc::now(),
            sync_status: SyncStatus::Pending,
        };

        self.store.add_validation(validation).await?;
        Ok(())
    }

    async fn task_grade(&self, _prompt: String) -> Result<TaskGradeResult> {
        // Task grading is not yet implemented for local memory
        Ok(TaskGradeResult {
            task_level: "unknown".to_string(),
            reason: "Task grading not yet implemented for local memory".to_string(),
            recommended_model: "default".to_string(),
            recommended_model_provider: None,
            confidence: 0.0,
        })
    }
}

/// Calculate freshness score from the last update timestamp.
///
/// Returns a value between 0 (very old) and 1 (recently updated).
/// Uses a 30-day half-life formula: 1 / (1 + days_old / 30)
fn calculate_freshness(updated_at: chrono::DateTime<chrono::Utc>) -> f32 {
    let now = chrono::Utc::now();
    let duration = now.signed_duration_since(updated_at);

    // Handle future dates (clock skew) - treat as fresh
    if duration.num_seconds() < 0 {
        return 1.0;
    }

    // Calculate days old (as f32)
    let days_old = duration.num_days() as f32;

    // Half-life of 30 days
    // - 0 days old = 1.0 (fresh)
    // - 30 days old = 0.5 (half fresh)
    // - 60 days old = 0.33
    // - 90+ days old = approaching 0
    1.0 / (1.0 + days_old / 30.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_local_memory_config() {
        let config = LocalMemoryConfig {
            db_path: "~/.memex/test".to_string(),
            embedding: EmbeddingConfig::Ollama {
                base_url: "http://localhost:11434".to_string(),
                model: "nomic-embed-text".to_string(),
                dimension: 768,
            },
            search_limit: 10,
            min_score: 0.3,
        };

        // Just verify it compiles
        assert_eq!(config.db_path, "~/.memex/test");
    }

    #[test]
    fn test_calculate_freshness() {
        let now = chrono::Utc::now();

        // Recent item (today) - should be very fresh
        let freshness_now = calculate_freshness(now);
        assert!((freshness_now - 1.0).abs() < 0.01);

        // 30 days old - should be 0.5 (half-life)
        let thirty_days_ago = now - chrono::Duration::days(30);
        let freshness_30 = calculate_freshness(thirty_days_ago);
        assert!((freshness_30 - 0.5).abs() < 0.01);

        // 60 days old - should be about 0.33
        let sixty_days_ago = now - chrono::Duration::days(60);
        let freshness_60 = calculate_freshness(sixty_days_ago);
        assert!((freshness_60 - 0.333).abs() < 0.01);

        // Future date (clock skew) - should be 1.0
        let future = now + chrono::Duration::days(1);
        let freshness_future = calculate_freshness(future);
        assert_eq!(freshness_future, 1.0);
    }
}

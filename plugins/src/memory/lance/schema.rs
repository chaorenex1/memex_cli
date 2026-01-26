//! LanceDB table schema definitions for local memory storage.
//!
//! This module defines the Arrow/LanceDB schemas for:
//! - QA items (questions + answers + embeddings)
//! - Validation records
//! - Hit records

use arrow_schema::{DataType, Field, Schema};
use std::sync::Arc;

/// Default embedding dimension for OpenAI text-embedding-3-small
pub const DEFAULT_EMBEDDING_DIM: usize = 4096;

/// Default embedding dimension for Ollama nomic-embed-text
pub const OLLAMA_EMBEDDING_DIM: usize = 4096;

/// Create the schema for the QA items table.
///
/// The schema includes:
/// - Metadata fields (id, project_id, timestamps)
/// - Content fields (question, answer)
/// - Vector field (question_vector for semantic search)
/// - Search/sync fields (tags, validation_level, sync_status)
pub fn qa_items_schema(embedding_dim: usize) -> Schema {
    let embedding_dim: i32 = embedding_dim
        .try_into()
        .expect("embedding dimension must fit in i32");
    Schema::new(vec![
        // Primary key
        Field::new("id", DataType::Utf8, false),
        Field::new("project_id", DataType::Utf8, false),
        // Content fields
        Field::new("question", DataType::Utf8, false),
        Field::new("answer", DataType::Utf8, false),
        // Vector column for semantic search
        Field::new(
            "question_vector",
            DataType::FixedSizeList(
                Arc::new(Field::new("item", DataType::Float32, true)),
                embedding_dim,
            ),
            true, // nullable
        ),
        // Metadata
        Field::new(
            "tags",
            DataType::List(Arc::new(Field::new("item", DataType::Utf8, true))),
            true,
        ),
        Field::new("confidence", DataType::Float32, true),
        Field::new("validation_level", DataType::UInt8, true), // 0=Candidate, 1=Verified, 2=Confirmed, 3=Gold
        Field::new("source", DataType::Utf8, true),
        Field::new("author", DataType::Utf8, true),
        Field::new("metadata", DataType::Utf8, true), // JSON string
        // Timestamps
        Field::new(
            "created_at",
            DataType::Timestamp(arrow_schema::TimeUnit::Millisecond, None),
            false,
        ),
        Field::new(
            "updated_at",
            DataType::Timestamp(arrow_schema::TimeUnit::Millisecond, None),
            false,
        ),
        // Sync state
        Field::new(
            "synced_at",
            DataType::Timestamp(arrow_schema::TimeUnit::Millisecond, None),
            true,
        ),
        Field::new("sync_status", DataType::Utf8, true), // "synced", "pending", "conflict", "local_only", "remote_only"
        Field::new("remote_id", DataType::Utf8, true),   // Remote server ID
        // Vectorization flag
        Field::new("is_vectorized", DataType::Boolean, false),
    ])
}

/// Create the schema for the validation records table.
pub fn validation_records_schema() -> Schema {
    Schema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("qa_id", DataType::Utf8, false),
        Field::new("result", DataType::Utf8, true), // "pass" | "fail" | "unknown"
        Field::new("signal_strength", DataType::Utf8, true), // "strong" | "weak"
        Field::new("success", DataType::Boolean, true),
        Field::new("context", DataType::Utf8, true), // JSON
        Field::new(
            "created_at",
            DataType::Timestamp(arrow_schema::TimeUnit::Millisecond, None),
            false,
        ),
        Field::new("sync_status", DataType::Utf8, true),
    ])
}

/// Create the schema for the hit records table.
pub fn hit_records_schema() -> Schema {
    Schema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("qa_id", DataType::Utf8, false),
        Field::new("shown", DataType::Boolean, false),
        Field::new("used", DataType::Boolean, true),
        Field::new("session_id", DataType::Utf8, true),
        Field::new(
            "created_at",
            DataType::Timestamp(arrow_schema::TimeUnit::Millisecond, None),
            false,
        ),
        Field::new("sync_status", DataType::Utf8, true),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_qa_items_schema_valid() {
        let schema = qa_items_schema(1536);
        assert_eq!(schema.fields().len(), 17);

        let field_names: Vec<_> = schema.fields().iter().map(|f| f.name().as_str()).collect();
        assert!(field_names.contains(&"id"));
        assert!(field_names.contains(&"question"));
        assert!(field_names.contains(&"answer"));
        assert!(field_names.contains(&"question_vector"));
    }

    #[test]
    fn test_validation_records_schema_valid() {
        let schema = validation_records_schema();
        assert_eq!(schema.fields().len(), 8);
    }

    #[test]
    fn test_hit_records_schema_valid() {
        let schema = hit_records_schema();
        assert_eq!(schema.fields().len(), 7);
    }
}

//! LanceDB-based local memory storage module.
//!
//! This module provides local-first memory storage using LanceDB,
//! with support for vector search and optional remote synchronization.

pub mod embedding;
pub mod local_embedding;
pub mod models;
pub mod schema;
pub mod store;

pub use embedding::{EmbeddingService, OllamaEmbeddingService, OpenAIEmbeddingService};
pub use local_embedding::{LocalEmbeddingConfig, LocalEmbeddingService};
pub use models::{
    HitRecord, QAItem, SignalStrength, SyncOperation, SyncStatus, ValidationLevel,
    ValidationRecord, ValidationResult,
};
pub use schema::{
    hit_records_schema, qa_items_schema, validation_records_schema, DEFAULT_EMBEDDING_DIM,
    OLLAMA_EMBEDDING_DIM,
};
pub use store::LanceStore;

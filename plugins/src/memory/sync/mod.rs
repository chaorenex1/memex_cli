//! Synchronization module for local and remote memory.
//!
//! Provides bidirectional synchronization between LanceDB local storage
//! and remote HTTP API with configurable conflict resolution.

pub mod conflict;
pub mod remote_client;
pub mod service;

pub use conflict::{resolve_conflict, ConflictRecord, ConflictResolutionResult};
pub use remote_client::{HttpRemoteMemoryClient, RemoteMemoryClient, RemoteQAItem, UploadResult};
pub use service::{SyncConfig, SyncEvent, SyncService, SyncStatusReport};

// Re-export ConflictResolution from core_api
pub use memex_core::api::ConflictResolution;

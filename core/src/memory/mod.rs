pub mod adapters;
pub mod models;
pub mod syncable;
pub mod r#trait;

mod candidates;
mod helpers;
mod payloads;
mod render;
mod types;

pub use r#trait::MemoryPlugin;
pub use syncable::{SyncStatusReport, SyncableMemory};

pub use adapters::parse_search_matches;
pub use models::{
    QACandidatePayload, QAHitsPayload, QAReferencePayload, QASearchPayload, QAValidationPayload,
};

pub use candidates::extract_candidates;
pub use payloads::{build_candidate_payloads, build_hit_payload, build_validate_payloads};
pub use render::{merge_prompt, render_memory_context};
pub use types::{CandidateDraft, CandidateExtractConfig, InjectConfig, InjectPlacement};

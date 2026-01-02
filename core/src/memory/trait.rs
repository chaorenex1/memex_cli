use crate::gatekeeper::{SearchMatch, TaskGradeResult};
use crate::memory::models::{
    QACandidatePayload, QAHitsPayload, QASearchPayload, QAValidationPayload,
};
use async_trait::async_trait;

#[async_trait]
pub trait MemoryPlugin: Send + Sync {
    fn name(&self) -> &str;
    async fn search(&self, payload: QASearchPayload) -> anyhow::Result<Vec<SearchMatch>>;
    async fn record_hit(&self, payload: QAHitsPayload) -> anyhow::Result<()>;
    async fn record_candidate(&self, payload: QACandidatePayload) -> anyhow::Result<()>;
    async fn record_validation(&self, payload: QAValidationPayload) -> anyhow::Result<()>;
    async fn task_grade(&self, prompt: String) -> anyhow::Result<TaskGradeResult>;
}

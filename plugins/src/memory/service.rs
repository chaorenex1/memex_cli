use super::http_client::HttpClient;
use super::r#trait::MemoryPlugin;
use anyhow::Result;
use async_trait::async_trait;
use memex_core::api as core_api;

pub struct MemoryServicePlugin {
    client: HttpClient,
}

impl MemoryServicePlugin {
    pub fn new(base_url: String, api_key: String, timeout_ms: u64) -> Result<Self> {
        let client = HttpClient::new(base_url, api_key, timeout_ms)?;
        Ok(Self { client })
    }
}

#[async_trait]
impl MemoryPlugin for MemoryServicePlugin {
    fn name(&self) -> &str {
        "memory_service"
    }

    async fn search(
        &self,
        payload: core_api::QASearchPayload,
    ) -> Result<Vec<core_api::SearchMatch>> {
        tracing::debug!(
            target: "memex.qa",
            stage = "memory.plugin.search.in",
            project_id = %payload.project_id,
            query_len = payload.query.len(),
            limit = payload.limit,
            min_score = payload.min_score
        );
        let raw = self.client.search(payload).await?;
        let out = core_api::parse_search_matches(&raw).map_err(|e: String| anyhow::anyhow!(e))?;
        tracing::debug!(
            target: "memex.qa",
            stage = "memory.plugin.search.out",
            matches = out.len()
        );
        Ok(out)
    }

    async fn record_hit(&self, payload: core_api::QAHitsPayload) -> Result<()> {
        let used = payload
            .references
            .iter()
            .filter(|r| r.used == Some(true))
            .count();
        let shown = payload
            .references
            .iter()
            .filter(|r| r.shown == Some(true))
            .count();
        tracing::debug!(
            target: "memex.qa",
            stage = "memory.plugin.hit.in",
            project_id = %payload.project_id,
            references = payload.references.len(),
            shown = shown,
            used = used
        );
        self.client.send_hit(payload).await?;
        tracing::debug!(target: "memex.qa", stage = "memory.plugin.hit.out");
        Ok(())
    }

    async fn record_candidate(&self, payload: core_api::QACandidatePayload) -> Result<()> {
        tracing::debug!(
            target: "memex.qa",
            stage = "memory.plugin.candidate.in",
            project_id = %payload.project_id,
            tags = payload.tags.len()
        );
        self.client.send_candidate(payload).await?;
        tracing::debug!(target: "memex.qa", stage = "memory.plugin.candidate.out");
        Ok(())
    }

    async fn record_validation(&self, payload: core_api::QAValidationPayload) -> Result<()> {
        tracing::debug!(
            target: "memex.qa",
            stage = "memory.plugin.validate.in",
            project_id = %payload.project_id,
            qa_id = %payload.qa_id,
            result = ?payload.result
        );
        self.client.send_validate(payload).await?;
        tracing::debug!(target: "memex.qa", stage = "memory.plugin.validate.out");
        Ok(())
    }

    async fn task_grade(&self, prompt: String) -> Result<core_api::TaskGradeResult> {
        tracing::debug!(
            target: "memex.task",
            stage = "memory.plugin.task_grade.in",
            prompt_len = prompt.len()
        );
        let raw = self.client.task_grade(prompt).await?;
        let out = serde_json::from_value::<core_api::TaskGradeResult>(raw)
            .map_err(|e| anyhow::anyhow!("Failed to parse TaskGradeResult: {}", e))?;
        tracing::debug!(
            target: "memex.task",
            stage = "memory.plugin.task_grade.out",
            grade = ?out.confidence
        );
        Ok(out)
    }
}

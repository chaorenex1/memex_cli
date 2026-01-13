use memex_core::api as core_api;
use serde_json::Value;
use std::{error::Error as StdError, fmt};

const BODY_PREVIEW_LIMIT: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryHttpErrorKind {
    Timeout,
    Connect,
    Request,
    Body,
    Decode,
    Status,
    Unknown,
}

impl MemoryHttpErrorKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Timeout => "timeout",
            Self::Connect => "connect",
            Self::Request => "request",
            Self::Body => "body",
            Self::Decode => "decode",
            Self::Status => "status",
            Self::Unknown => "unknown",
        }
    }
}

impl fmt::Display for MemoryHttpErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug)]
pub struct MemoryHttpError {
    kind: MemoryHttpErrorKind,
    status: Option<u16>,
    url: Option<String>,
    message: String,
    source: Option<anyhow::Error>,
}

impl MemoryHttpError {
    pub fn kind(&self) -> MemoryHttpErrorKind {
        self.kind
    }

    pub fn status(&self) -> Option<u16> {
        self.status
    }

    pub fn url(&self) -> Option<&str> {
        self.url.as_deref()
    }

    fn from_reqwest(err: reqwest::Error, url: String) -> Self {
        let kind = if err.is_timeout() {
            MemoryHttpErrorKind::Timeout
        } else if err.is_connect() {
            MemoryHttpErrorKind::Connect
        } else if err.is_request() {
            MemoryHttpErrorKind::Request
        } else if err.is_body() {
            MemoryHttpErrorKind::Body
        } else if err.is_decode() {
            MemoryHttpErrorKind::Decode
        } else {
            MemoryHttpErrorKind::Unknown
        };
        let status = err.status().map(|s| s.as_u16());
        let message = err.to_string();
        MemoryHttpError {
            kind,
            status,
            url: Some(url),
            message,
            source: Some(anyhow::Error::new(err)),
        }
    }

    fn status_error(status: u16, url: String, preview: String) -> Self {
        MemoryHttpError {
            kind: MemoryHttpErrorKind::Status,
            status: Some(status),
            url: Some(url),
            message: preview,
            source: None,
        }
    }

    fn decode_error(status: u16, url: String, err: serde_json::Error, preview: String) -> Self {
        let message = format!("failed to decode response body: {} | body={}", err, preview);
        MemoryHttpError {
            kind: MemoryHttpErrorKind::Decode,
            status: Some(status),
            url: Some(url),
            message,
            source: Some(anyhow::Error::new(err)),
        }
    }
}

impl fmt::Display for MemoryHttpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "memory http error kind={}", self.kind)?;
        if let Some(status) = self.status {
            write!(f, " status={}", status)?;
        }
        if let Some(url) = &self.url {
            write!(f, " url={}", url)?;
        }
        write!(f, ": {}", self.message)
    }
}

impl StdError for MemoryHttpError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        self.source
            .as_ref()
            .map(|err| &**err as &(dyn StdError + 'static))
    }
}

fn preview_body(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return "<empty body>".to_string();
    }

    let mut out = String::new();
    let mut truncated = false;
    for (idx, ch) in trimmed.chars().enumerate() {
        if idx >= BODY_PREVIEW_LIMIT {
            truncated = true;
            break;
        }
        out.push(ch);
    }

    if truncated {
        out.push_str("...");
    }

    out
}

async fn parse_json_response(resp: reqwest::Response) -> anyhow::Result<Value> {
    let status = resp.status();
    let url = resp.url().to_string();
    let body = resp
        .text()
        .await
        .map_err(|err| MemoryHttpError::from_reqwest(err, url.clone()))?;

    if !status.is_success() {
        let preview = preview_body(&body);
        return Err(MemoryHttpError::status_error(status.as_u16(), url, preview).into());
    }

    if body.trim().is_empty() {
        return Ok(Value::Null);
    }

    serde_json::from_str::<Value>(&body).map_err(|err| {
        let preview = preview_body(&body);
        MemoryHttpError::decode_error(status.as_u16(), url, err, preview).into()
    })
}

async fn ensure_success(resp: reqwest::Response) -> anyhow::Result<()> {
    let status = resp.status();
    let url = resp.url().to_string();

    if status.is_success() {
        return Ok(());
    }

    let body = resp
        .text()
        .await
        .map_err(|err| MemoryHttpError::from_reqwest(err, url.clone()))?;
    let preview = preview_body(&body);
    Err(MemoryHttpError::status_error(status.as_u16(), url, preview).into())
}

#[derive(Clone)]
pub struct HttpClient {
    api_key: String,
    http: reqwest::Client,
    // Pre-built URL endpoints for performance (avoid repeated format! and trim)
    url_search: String,
    url_hit: String,
    url_candidate: String,
    url_validate: String,
    url_task_grade: String,
}

impl HttpClient {
    pub fn new(base_url: String, api_key: String, timeout_ms: u64) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(timeout_ms))
            .build()?;
        let normalized = base_url.trim_end_matches('/');
        Ok(Self {
            api_key,
            http,
            url_search: format!("{}/v1/qa/search", normalized),
            url_hit: format!("{}/v1/qa/hit", normalized),
            url_candidate: format!("{}/v1/qa/candidates", normalized),
            url_validate: format!("{}/v1/qa/validate", normalized),
            url_task_grade: format!("{}/v1/task/grade", normalized),
        })
    }

    fn auth(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if self.api_key.trim().is_empty() {
            req
        } else {
            req.bearer_auth(&self.api_key)
        }
    }

    pub async fn search(&self, payload: core_api::QASearchPayload) -> anyhow::Result<Value> {
        let url = &self.url_search;
        tracing::debug!(
            target: "memex.qa",
            stage = "memory.http.search.in",
            url = %url,
            project_id = %payload.project_id,
            query_len = payload.query.len(),
            limit = payload.limit,
            min_score = payload.min_score
        );
        let req = self.http.post(url).json(&payload);
        let resp = self
            .auth(req)
            .send()
            .await
            .map_err(|err| MemoryHttpError::from_reqwest(err, url.clone()))?;
        let status = resp.status();
        let v = parse_json_response(resp).await?;
        tracing::debug!(
            target: "memex.qa",
            stage = "memory.http.search.out",
            status = %status
        );
        Ok(v)
    }

    pub async fn send_hit(&self, payload: core_api::QAHitsPayload) -> anyhow::Result<()> {
        let url = &self.url_hit;
        // Single-pass counting for used and shown references
        let (used, shown) = payload.references.iter().fold((0, 0), |(u, s), r| {
            (
                u + usize::from(r.used == Some(true)),
                s + usize::from(r.shown == Some(true)),
            )
        });
        tracing::debug!(
            target: "memex.qa",
            stage = "memory.http.hit.in",
            url = %url,
            project_id = %payload.project_id,
            references = payload.references.len(),
            shown = shown,
            used = used
        );
        let req = self.http.post(url).json(&payload);
        let resp = self
            .auth(req)
            .send()
            .await
            .map_err(|err| MemoryHttpError::from_reqwest(err, url.clone()))?;
        let status = resp.status();
        ensure_success(resp).await?;
        tracing::debug!(target: "memex.qa", stage = "memory.http.hit.out", status = %status);
        Ok(())
    }

    pub async fn send_candidate(
        &self,
        payload: core_api::QACandidatePayload,
    ) -> anyhow::Result<()> {
        let url = &self.url_candidate;
        tracing::debug!(
            target: "memex.qa",
            stage = "memory.http.candidate.in",
            url = %url,
            project_id = %payload.project_id,
            tags = payload.tags.len()
        );
        let req = self.http.post(url).json(&payload);
        let resp = self
            .auth(req)
            .send()
            .await
            .map_err(|err| MemoryHttpError::from_reqwest(err, url.clone()))?;
        let status = resp.status();
        ensure_success(resp).await?;
        tracing::debug!(
            target: "memex.qa",
            stage = "memory.http.candidate.out",
            status = %status
        );
        Ok(())
    }

    pub async fn send_validate(
        &self,
        payload: core_api::QAValidationPayload,
    ) -> anyhow::Result<()> {
        let url = &self.url_validate;
        tracing::debug!(
            target: "memex.qa",
            stage = "memory.http.validate.in",
            url = %url,
            project_id = %payload.project_id,
            qa_id = %payload.qa_id,
            result = ?payload.result
        );
        let req = self.http.post(url).json(&payload);
        let resp = self
            .auth(req)
            .send()
            .await
            .map_err(|err| MemoryHttpError::from_reqwest(err, url.clone()))?;
        let status = resp.status();
        ensure_success(resp).await?;
        tracing::debug!(
            target: "memex.qa",
            stage = "memory.http.validate.out",
            status = %status
        );
        Ok(())
    }

    pub async fn task_grade(&self, prompt: String) -> anyhow::Result<Value> {
        let url = &self.url_task_grade;
        tracing::debug!(
            target: "memex.task",
            stage = "memory.http.task_grade.in",
            url = %url
        );
        let req = self
            .http
            .post(url)
            .json(&serde_json::json!({ "prompt": prompt }));
        let resp = self
            .auth(req)
            .send()
            .await
            .map_err(|err| MemoryHttpError::from_reqwest(err, url.clone()))?;
        let status = resp.status();
        let v = parse_json_response(resp).await?;
        tracing::debug!(
            target: "memex.task",
            stage = "memory.http.task_grade.out",
            status = %status
        );
        Ok(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::Matcher;
    use mockito::Server;

    #[test]
    fn test_preview_body_empty() {
        assert_eq!(preview_body("   "), "<empty body>");
    }

    #[test]
    fn test_preview_body_truncates() {
        let body = "a".repeat(BODY_PREVIEW_LIMIT + 10);
        let preview = preview_body(&body);
        assert!(preview.ends_with("..."));
        assert!(preview.len() <= BODY_PREVIEW_LIMIT + 3);
    }

    #[test]
    fn test_memory_http_error_display_status() {
        let err = MemoryHttpError::status_error(
            502,
            "https://example.com/v1/qa/candidates".to_string(),
            "bad gateway".to_string(),
        );
        let msg = err.to_string();
        assert!(msg.contains("kind=status"));
        assert!(msg.contains("status=502"));
        assert!(msg.contains("url=https://example.com/v1/qa/candidates"));
        assert!(msg.contains("bad gateway"));
    }

    #[test]
    fn test_memory_http_error_display_decode() {
        let decode_err = serde_json::from_str::<Value>("not json").unwrap_err();
        let err = MemoryHttpError::decode_error(
            200,
            "https://example.com/v1/qa/search".to_string(),
            decode_err,
            "not json".to_string(),
        );
        let msg = err.to_string();
        assert!(msg.contains("kind=decode"));
        assert!(msg.contains("status=200"));
        assert!(msg.contains("url=https://example.com/v1/qa/search"));
        assert!(msg.contains("failed to decode response body"));
    }

    #[tokio::test]
    async fn test_search_returns_json() {
        let mut server = Server::new_async().await;
        let _m = server
            .mock("POST", "/v1/qa/search")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[{"qa_id":"id","question":"Q","answer":"A"}]"#)
            .create_async()
            .await;

        let client = HttpClient::new(server.url(), "".to_string(), 1_000).unwrap();
        let payload = core_api::QASearchPayload {
            project_id: "proj".to_string(),
            query: "query".to_string(),
            limit: 5,
            min_score: 0.6,
        };
        let value = client.search(payload).await.unwrap();
        assert!(value.is_array());
    }

    #[tokio::test]
    async fn test_send_candidate_accepts_empty_body() {
        let mut server = Server::new_async().await;
        let _m = server
            .mock("POST", "/v1/qa/candidates")
            .with_status(204)
            .create_async()
            .await;

        let client = HttpClient::new(server.url(), "".to_string(), 1_000).unwrap();
        let payload = core_api::QACandidatePayload {
            project_id: "proj".to_string(),
            question: "Q".to_string(),
            answer: "A".to_string(),
            tags: vec![],
            confidence: 0.0,
            metadata: serde_json::json!({}),
            summary: None,
            source: None,
            author: None,
        };
        client.send_candidate(payload).await.unwrap();
    }

    #[tokio::test]
    async fn test_send_hit_accepts_empty_body() {
        let mut server = Server::new_async().await;
        let _m = server
            .mock("POST", "/v1/qa/hit")
            .with_status(204)
            .create_async()
            .await;

        let client = HttpClient::new(server.url(), "".to_string(), 1_000).unwrap();
        let payload = core_api::QAHitsPayload {
            project_id: "proj".to_string(),
            references: vec![core_api::QAReferencePayload {
                qa_id: "qa1".to_string(),
                shown: None,
                used: Some(true),
                message_id: None,
                context: None,
            }],
        };
        client.send_hit(payload).await.unwrap();
    }

    #[tokio::test]
    async fn test_send_validate_accepts_empty_body() {
        let mut server = Server::new_async().await;
        let _m = server
            .mock("POST", "/v1/qa/validate")
            .with_status(204)
            .create_async()
            .await;

        let client = HttpClient::new(server.url(), "".to_string(), 1_000).unwrap();
        let payload = core_api::QAValidationPayload {
            project_id: "proj".to_string(),
            qa_id: "qa1".to_string(),
            result: Some("success".to_string()),
            signal_strength: None,
            success: Some(true),
            strong_signal: Some(true),
            source: None,
            context: None,
            client: None,
            ts: None,
            payload: None,
        };
        client.send_validate(payload).await.unwrap();
    }

    #[tokio::test]
    async fn test_send_candidate_status_error() {
        let mut server = Server::new_async().await;
        let _m = server
            .mock("POST", "/v1/qa/candidates")
            .with_status(502)
            .with_body("bad gateway")
            .create_async()
            .await;

        let client = HttpClient::new(server.url(), "".to_string(), 1_000).unwrap();
        let payload = core_api::QACandidatePayload {
            project_id: "proj".to_string(),
            question: "Q".to_string(),
            answer: "A".to_string(),
            tags: vec![],
            confidence: 0.0,
            metadata: serde_json::json!({}),
            summary: None,
            source: None,
            author: None,
        };

        let err = client.send_candidate(payload).await.unwrap_err();
        let mem_err = err
            .downcast_ref::<MemoryHttpError>()
            .expect("expected MemoryHttpError");
        assert_eq!(mem_err.kind(), MemoryHttpErrorKind::Status);
        assert_eq!(mem_err.status(), Some(502));
        assert!(mem_err
            .url()
            .unwrap_or_default()
            .contains("/v1/qa/candidates"));
    }

    #[tokio::test]
    async fn test_task_grade_decode_error() {
        let mut server = Server::new_async().await;
        let _m = server
            .mock("POST", "/v1/task/grade")
            .with_status(200)
            .with_body("not json")
            .create_async()
            .await;

        let client = HttpClient::new(server.url(), "".to_string(), 1_000).unwrap();
        let err = client.task_grade("prompt".to_string()).await.unwrap_err();
        let mem_err = err
            .downcast_ref::<MemoryHttpError>()
            .expect("expected MemoryHttpError");
        assert_eq!(mem_err.kind(), MemoryHttpErrorKind::Decode);
        assert_eq!(mem_err.status(), Some(200));
        assert!(mem_err.url().unwrap_or_default().contains("/v1/task/grade"));
    }

    #[tokio::test]
    async fn test_auth_header_included_when_api_key_set() {
        let mut server = Server::new_async().await;
        let _m = server
            .mock("POST", "/v1/qa/hit")
            .match_header("authorization", "Bearer secret-token")
            .with_status(204)
            .create_async()
            .await;

        let client = HttpClient::new(server.url(), "secret-token".to_string(), 1_000).unwrap();
        let payload = core_api::QAHitsPayload {
            project_id: "proj".to_string(),
            references: vec![core_api::QAReferencePayload {
                qa_id: "qa1".to_string(),
                shown: None,
                used: Some(true),
                message_id: None,
                context: None,
            }],
        };
        client.send_hit(payload).await.unwrap();
    }

    #[tokio::test]
    async fn test_auth_header_absent_when_api_key_empty() {
        let mut server = Server::new_async().await;
        let _m = server
            .mock("POST", "/v1/qa/hit")
            .match_header("authorization", Matcher::Missing)
            .with_status(204)
            .create_async()
            .await;

        let client = HttpClient::new(server.url(), "".to_string(), 1_000).unwrap();
        let payload = core_api::QAHitsPayload {
            project_id: "proj".to_string(),
            references: vec![core_api::QAReferencePayload {
                qa_id: "qa1".to_string(),
                shown: None,
                used: Some(true),
                message_id: None,
                context: None,
            }],
        };
        client.send_hit(payload).await.unwrap();
    }
}

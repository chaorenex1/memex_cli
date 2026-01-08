use serde::Deserialize;
use serde_json::Value;

use crate::gatekeeper::SearchMatch;

#[derive(Debug, Clone, Deserialize)]
struct SearchMatchCompat {
    pub qa_id: String,

    #[serde(default)]
    pub project_id: String,

    #[serde(default)]
    pub question: String,

    #[serde(default)]
    pub answer: String,

    #[serde(default)]
    pub summary: Option<String>,

    #[serde(default)]
    pub score: f32,

    #[serde(default)]
    pub relevance: f32,

    #[serde(default)]
    pub trust: f32,

    #[serde(default)]
    pub freshness: f32,

    #[serde(default)]
    pub validation_level: i32,

    #[serde(default)]
    pub level: Option<String>,

    #[serde(default = "default_status")]
    pub status: String,

    #[serde(default, alias = "meta")]
    pub metadata: serde_json::Value,

    #[serde(default)]
    pub tags: Vec<String>,

    #[serde(default)]
    pub expiry_at: Option<String>,

    #[serde(default)]
    pub source: Option<String>,

    #[serde(default)]
    pub confidence: Option<f32>,
}

fn default_status() -> String {
    "active".to_string()
}

impl From<SearchMatchCompat> for SearchMatch {
    fn from(c: SearchMatchCompat) -> Self {
        SearchMatch {
            qa_id: c.qa_id,
            project_id: Some(c.project_id),
            question: c.question,
            answer: c.answer,
            summary: c.summary,
            score: c.score,
            relevance: c.relevance,
            trust: c.trust,
            freshness: c.freshness,
            validation_level: c.validation_level,
            level: c.level,
            status: c.status,
            metadata: c.metadata,
            tags: c.tags,
            expiry_at: c.expiry_at,
            source: c.source,
            confidence: c.confidence.unwrap_or(0.0),
        }
    }
}

pub fn parse_search_matches(v: &Value) -> Result<Vec<SearchMatch>, String> {
    let arr = v
        .as_array()
        .ok_or_else(|| {
            // 检查是否为错误响应
            if let Some(err_msg) = v.get("error").and_then(|e| e.as_str()) {
                tracing::warn!(
                    target: "memex.qa",
                    "API returned error response: {}",
                    err_msg
                );
                return format!("API returned error: {}", err_msg);
            }
            let response_preview = serde_json::to_string(v)
                .unwrap_or_else(|_| "non-serializable".to_string());
            tracing::error!(
                target: "memex.qa",
                "Invalid API response format: expected array, got {}",
                response_preview
            );
            format!("search response must be top-level array (List[Dict]), got: {}",
                    response_preview)
        })?;

    tracing::debug!(
        target: "memex.qa",
        "Parsing {} search result items",
        arr.len()
    );

    let mut out = Vec::with_capacity(arr.len());
    let mut errs: Vec<String> = Vec::new();

    for (i, item) in arr.iter().enumerate() {
        match serde_json::from_value::<SearchMatchCompat>(item.clone()) {
            Ok(x) => out.push(x.into()),
            Err(e) => {
                let preview = serde_json::to_string(item).unwrap_or_else(|_| "?".to_string());
                let preview_truncated = if preview.len() > 200 {
                    format!("{}...", &preview[..200])
                } else {
                    preview
                };
                tracing::warn!(
                    target: "memex.qa",
                    "Failed to parse search item #{}: {} | Data: {}",
                    i,
                    e,
                    preview_truncated
                );
                errs.push(format!("#{}: {}", i, e))
            },
        }
    }

    // 区分"空结果"和"解析失败"
    if errs.is_empty() {
        // 没有解析错误：空结果或全部成功
        tracing::debug!(
            target: "memex.qa",
            "Successfully parsed {} search items",
            out.len()
        );
        Ok(out)
    } else if !out.is_empty() {
        // 部分成功：有成功的也有失败的
        tracing::warn!(
            target: "memex.qa",
            "Partial parse success: {} succeeded, {} failed",
            out.len(),
            errs.len()
        );
        Ok(out)
    } else {
        // 全部失败：有条目但都无法解析
        tracing::error!(
            target: "memex.qa",
            "Failed to parse all {} search items",
            arr.len()
        );
        Err(format!(
            "failed to parse all {} search items: {}",
            arr.len(),
            errs.join("; ")
        ))
    }
}

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
        .ok_or("search response must be top-level array (List[Dict])")?;

    let mut out = Vec::with_capacity(arr.len());
    let mut errs: Vec<String> = Vec::new();

    for (i, item) in arr.iter().enumerate() {
        match serde_json::from_value::<SearchMatchCompat>(item.clone()) {
            Ok(x) => out.push(x.into()),
            Err(e) => errs.push(format!("#{}: {}", i, e)),
        }
    }

    if !out.is_empty() {
        Ok(out)
    } else {
        Err(format!(
            "failed to parse all search items: {}",
            errs.join("; ")
        ))
    }
}

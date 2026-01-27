use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateDraft {
    pub question: String,
    pub answer: String,
    pub tags: Vec<String>,
    pub confidence: f32,
    pub metadata: Value,
    pub summary: Option<String>,
    pub source: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum InjectPlacement {
    System,
    User,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InjectConfig {
    pub placement: InjectPlacement,
    pub max_items: usize,
    pub max_answer_chars: usize,
    pub include_meta_line: bool,
}

impl Default for InjectConfig {
    fn default() -> Self {
        Self {
            placement: InjectPlacement::System,
            max_items: 3,
            max_answer_chars: 900,
            include_meta_line: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateExtractConfig {
    pub max_candidates: usize,
    pub max_answer_chars: usize,
    pub min_answer_chars: usize,
    pub context_lines: usize,
    pub tool_steps_max: usize,
    pub tool_step_args_keys_max: usize,
    pub tool_step_value_max_chars: usize,
    pub redact: bool,
    pub strict_secret_block: bool,
    pub confidence: f32,
}

impl Default for CandidateExtractConfig {
    fn default() -> Self {
        Self {
            max_candidates: 1,
            max_answer_chars: 1200,
            min_answer_chars: 200,
            context_lines: 8,
            tool_steps_max: 5,
            tool_step_args_keys_max: 16,
            tool_step_value_max_chars: 140,
            redact: true,
            strict_secret_block: true,
            confidence: 0.45,
        }
    }
}

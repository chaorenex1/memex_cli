use std::fmt::Write;

use crate::gatekeeper::InjectItem;

use super::helpers::{one_line, truncate_clean};
use super::types::InjectConfig;

/// Render memory context for prompt injection. Optimized to minimize allocations.
pub fn render_memory_context(items: &[InjectItem], cfg: &InjectConfig) -> String {
    if items.is_empty() {
        return String::new();
    }

    // Pre-allocate estimated capacity to avoid reallocations
    let mut out = String::with_capacity(items.len() * 500);
    out.push_str("[MEMORY_CONTEXT v1]\n");
    out.push_str("The following items are retrieved from the memory system. Prefer using them when relevant.\n");
    out.push_str("If you use an item, include its anchor exactly once in your final answer: [QA_REF <qa_id>].\n\n");

    for (idx, it) in items.iter().take(cfg.max_items).enumerate() {
        let n = idx + 1;
        // Use write! macro to avoid intermediate String allocations
        let _ = writeln!(out, "{n}) [QA_REF {}]", it.qa_id);
        let _ = writeln!(out, "Q: {}", one_line(&it.question));
        let a = pick_answer(it, cfg.max_answer_chars);
        let _ = writeln!(out, "A: {}", a);

        if cfg.include_meta_line {
            let tags_str = if it.tags.is_empty() {
                "-"
            } else {
                // Only join when needed, avoid allocation if tags is empty
                &it.tags.join(",")
            };
            let _ = writeln!(
                out,
                "Meta: level={} trust={:.2} score={:.2} tags={}",
                it.validation_level, it.trust, it.score, tags_str
            );
        }
        out.push('\n');
    }

    out.push_str("Rules:\n");
    out.push_str("- Do not invent anchors.\n");
    out.push_str("- If none are relevant, ignore them.\n");
    out.push_str("- Prefer the highest validation_level and trust.\n");
    out.push_str("[/MEMORY_CONTEXT]\n");

    out
}

pub fn merge_prompt(user_query: &str, memory_context: &str) -> String {
    if memory_context.trim().is_empty() {
        return user_query.to_string();
    }
    format!("{memory_context}\n{user_query}")
}

fn pick_answer(it: &InjectItem, max_chars: usize) -> String {
    let raw = if let Some(s) = &it.summary {
        s.as_str()
    } else {
        it.answer.as_str()
    };
    truncate_clean(raw, max_chars)
}

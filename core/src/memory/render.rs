use crate::gatekeeper::InjectItem;

use super::helpers::{one_line, truncate_clean};
use super::types::InjectConfig;

pub fn render_memory_context(items: &[InjectItem], cfg: &InjectConfig) -> String {
    if items.is_empty() {
        return String::new();
    }

    let mut out = String::new();
    out.push_str("[MEMORY_CONTEXT v1]\n");
    out.push_str("The following items are retrieved from the memory system. Prefer using them when relevant.\n");
    out.push_str("If you use an item, include its anchor exactly once in your final answer: [QA_REF <qa_id>].\n\n");

    for (idx, it) in items.iter().take(cfg.max_items).enumerate() {
        let n = idx + 1;
        out.push_str(&format!("{n}) [QA_REF {}]\n", it.qa_id));
        out.push_str(&format!("Q: {}\n", one_line(&it.question)));
        let a = pick_answer(it, cfg.max_answer_chars);
        out.push_str(&format!("A: {}\n", a));

        if cfg.include_meta_line {
            out.push_str(&format!(
                "Meta: level={} trust={:.2} score={:.2} tags={}\n",
                it.validation_level,
                it.trust,
                it.score,
                if it.tags.is_empty() {
                    "-".to_string()
                } else {
                    it.tags.join(",")
                }
            ));
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

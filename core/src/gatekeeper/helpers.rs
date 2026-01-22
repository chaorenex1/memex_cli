use std::collections::BTreeSet;
use std::sync::OnceLock;

use regex::Regex;

// Cached regex for QA_REF extraction (compiled once, reused forever)
static QA_REF_REGEX: OnceLock<Regex> = OnceLock::new();

fn qa_ref_regex() -> &'static Regex {
    QA_REF_REGEX.get_or_init(|| {
        Regex::new(r"\[QA_REF\s+([A-Za-z0-9_\-]+)\]").expect("QA_REF_REGEX is valid")
    })
}

pub fn extract_qa_refs(text: &str) -> Vec<String> {
    let re = qa_ref_regex();
    let mut set = BTreeSet::new();

    for cap in re.captures_iter(text) {
        if let Some(m) = cap.get(1) {
            set.insert(m.as_str().to_string());
        }
    }

    set.into_iter().collect()
}

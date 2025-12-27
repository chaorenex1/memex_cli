use regex::Regex;
use std::collections::BTreeSet;

pub fn extract_qa_refs(text: &str) -> Vec<String> {
    let re = Regex::new(r"\[QA_REF\s+([A-Za-z0-9_\-]+)\]").expect("valid regex");
    let mut set = BTreeSet::new();

    for cap in re.captures_iter(text) {
        if let Some(m) = cap.get(1) {
            set.insert(m.as_str().to_string());
        }
    }

    set.into_iter().collect()
}

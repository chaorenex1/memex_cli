use chrono::Local;
use uuid::Uuid;

/// 生成格式: task-{YYYYMMDDHHmmss}-{random8}
pub fn generate_task_id() -> String {
    let ts = Local::now().format("%Y%m%d%H%M%S");
    let uuid = Uuid::new_v4().simple().to_string();
    let suffix = &uuid[..8];
    format!("task-{}-{}", ts, suffix)
}

#[cfg(test)]
mod tests {
    use super::*;
    use regex::Regex;
    use std::collections::HashSet;

    #[test]
    fn test_generate_task_id_format() {
        let id = generate_task_id();
        let re = Regex::new(r"^task-\d{14}-[a-f0-9]{8}$").unwrap();
        assert!(re.is_match(&id), "Generated ID: {}", id);
    }

    #[test]
    fn test_generate_task_id_uniqueness() {
        let mut ids = HashSet::new();
        for _ in 0..200 {
            let id = generate_task_id();
            assert!(ids.insert(id.clone()), "Duplicate ID: {}", id);
        }
    }
}

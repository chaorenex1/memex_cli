//! 基础请求验证逻辑

use super::models::HttpServerError;

/// 验证record-candidate请求的基础字段
pub fn validate_candidate(question: &str, answer: &str) -> Result<(), HttpServerError> {
    // 验证问题长度
    let question_trimmed = question.trim();
    if question_trimmed.is_empty() {
        return Err(HttpServerError::InvalidRequest(
            "Question cannot be empty".to_string(),
        ));
    }
    if question_trimmed.len() < 5 {
        return Err(HttpServerError::InvalidRequest(format!(
            "Question too short ({} chars, min 5)",
            question_trimmed.len()
        )));
    }
    if question_trimmed.len() > 100000 {
        return Err(HttpServerError::InvalidRequest(format!(
            "Question too long ({} chars, max 100000)",
            question_trimmed.len()
        )));
    }

    // 验证答案长度
    let answer_trimmed = answer.trim();
    if answer_trimmed.is_empty() {
        return Err(HttpServerError::InvalidRequest(
            "Answer cannot be empty".to_string(),
        ));
    }
    if answer_trimmed.len() < 10 {
        return Err(HttpServerError::InvalidRequest(format!(
            "Answer too short ({} chars, min 10)",
            answer_trimmed.len()
        )));
    }
    if answer_trimmed.len() > 100000 {
        return Err(HttpServerError::InvalidRequest(format!(
            "Answer too long ({} chars, max 100000)",
            answer_trimmed.len()
        )));
    }

    Ok(())
}

/// 验证project_id格式（仅允许字母数字、下划线、连字符）
pub fn validate_project_id(project_id: &str) -> Result<(), HttpServerError> {
    if project_id.is_empty() {
        return Err(HttpServerError::InvalidRequest(
            "Project ID cannot be empty".to_string(),
        ));
    }

    if project_id.len() > 100 {
        return Err(HttpServerError::InvalidRequest(format!(
            "Project ID too long ({} chars, max 100)",
            project_id.len()
        )));
    }

    // 仅允许字母数字、下划线、连字符
    if !project_id
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
    {
        return Err(HttpServerError::InvalidRequest(
            "Project ID can only contain alphanumeric, underscore, and hyphen characters"
                .to_string(),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_candidate_success() {
        let result = validate_candidate("What is Rust?", "Rust is a systems programming language");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_candidate_empty_question() {
        let result = validate_candidate("", "Answer");
        assert!(result.is_err());
        match result {
            Err(HttpServerError::InvalidRequest(msg)) => {
                assert!(msg.contains("empty"));
            }
            _ => panic!("Expected InvalidRequest error"),
        }
    }

    #[test]
    fn test_validate_candidate_question_too_short() {
        let result = validate_candidate("Hi", "This is a long answer");
        assert!(result.is_err());
        match result {
            Err(HttpServerError::InvalidRequest(msg)) => {
                assert!(msg.contains("too short"));
                assert!(msg.contains("min 5"));
            }
            _ => panic!("Expected InvalidRequest error"),
        }
    }

    #[test]
    fn test_validate_candidate_answer_too_short() {
        let result = validate_candidate("What is this?", "Short");
        assert!(result.is_err());
        match result {
            Err(HttpServerError::InvalidRequest(msg)) => {
                assert!(msg.contains("too short"));
                assert!(msg.contains("min 10"));
            }
            _ => panic!("Expected InvalidRequest error"),
        }
    }

    #[test]
    fn test_validate_candidate_boundary_values() {
        // 边界值测试：问题刚好5字符
        assert!(validate_candidate("12345", "Answer is here").is_ok());

        // 边界值测试：答案刚好10字符
        assert!(validate_candidate("Question?", "1234567890").is_ok());
    }

    #[test]
    fn test_validate_project_id_success() {
        assert!(validate_project_id("memex-cli").is_ok());
        assert!(validate_project_id("project_123").is_ok());
        assert!(validate_project_id("my-project-2024").is_ok());
    }

    #[test]
    fn test_validate_project_id_empty() {
        let result = validate_project_id("");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_project_id_invalid_chars() {
        let result = validate_project_id("project@name");
        assert!(result.is_err());
        match result {
            Err(HttpServerError::InvalidRequest(msg)) => {
                assert!(msg.contains("alphanumeric"));
            }
            _ => panic!("Expected InvalidRequest error"),
        }
    }

    #[test]
    fn test_validate_project_id_too_long() {
        let long_id = "a".repeat(101);
        let result = validate_project_id(&long_id);
        assert!(result.is_err());
        match result {
            Err(HttpServerError::InvalidRequest(msg)) => {
                assert!(msg.contains("too long"));
            }
            _ => panic!("Expected InvalidRequest error"),
        }
    }
}

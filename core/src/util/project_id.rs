//! Project ID generation utilities
//!
//! Provides cross-platform path normalization for generating consistent project IDs.
use std::path::Path;

/// Generate project_id from directory path
///
/// Format: Normalized path string (pure path format)
/// Example: c--users-user-projects-memex_cli
///
/// Rules:
/// 1. Convert to lowercase
/// 2. Windows drive "C:\\" → "c--"
/// 3. Unix: remove leading "/"
/// 4. Path separators / and \\ → "-"
/// 5. Spaces and special chars → "_"
/// 6. Maximum length 64 chars
///
pub fn generate_project_id_str(path: &str) -> String {
    generate_project_id(Path::new(path))
}

pub fn generate_project_id(path: &Path) -> String {
    let path_str = path.to_string_lossy().to_string();

    if path_str.is_empty() {
        return "default".to_string();
    }

    // Normalize to lowercase
    let normalized = path_str.to_lowercase();

    // Check for Windows drive letter
    let (drive_letter, rest_path) =
        if normalized.len() >= 2 && normalized.chars().nth(1) == Some(':') {
            let drive = normalized.chars().next().unwrap();
            let rest = if normalized.len() > 3 {
                &normalized[3..] // Skip "C:\\" or "C:/"
            } else {
                ""
            };
            (Some(drive), rest.to_string())
        } else if let Some(stripped) = normalized.strip_prefix('/') {
            // Unix: remove leading "/"
            (None, stripped.to_string())
        } else {
            // Relative path
            (None, normalized)
        };

    // Replace path separators with "-"
    let mut sanitized = rest_path.replace(['\\', '/'], "-");

    // Sanitize (replace special chars, limit length, etc.)
    sanitized = sanitize_project_id(&sanitized);

    // Prepend drive letter if present
    if let Some(drive) = drive_letter {
        if !sanitized.is_empty() {
            format!("{}--{}", drive, sanitized)
        } else {
            format!("{}--", drive)
        }
    } else if sanitized.is_empty() {
        "default".to_string()
    } else {
        sanitized
    }
}

/// Sanitize project_id to ensure it meets requirements
///
/// Rules:
/// - Keep only letters, numbers, hyphens, underscores
/// - Convert to lowercase
/// - Remove consecutive special chars
/// - Strip leading/trailing special chars
/// - Limit to 64 characters
fn sanitize_project_id(raw_id: &str) -> String {
    if raw_id.is_empty() {
        return "default".to_string();
    }

    // Replace non-alphanumeric chars (except - and _) with "_"
    let mut sanitized: String = raw_id
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();

    // Convert to lowercase
    sanitized = sanitized.to_lowercase();

    // Remove consecutive underscores
    while sanitized.contains("__") {
        sanitized = sanitized.replace("__", "_");
    }

    // Remove consecutive hyphens
    while sanitized.contains("--") {
        sanitized = sanitized.replace("--", "-");
    }

    // Strip leading/trailing special chars
    sanitized = sanitized
        .trim_matches(|c: char| c == '_' || c == '-')
        .to_string();

    // Limit length
    if sanitized.len() > 64 {
        sanitized.truncate(64);
    }

    if sanitized.is_empty() {
        "default".to_string()
    } else {
        sanitized
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_windows_path() {
        let path = PathBuf::from(r"C:\Users\zarag\Documents\aduib-app\memex_cli");
        assert_eq!(
            generate_project_id(&path),
            "c--users-zarag-documents-aduib-app-memex_cli"
        );
    }

    #[test]
    fn test_unix_path() {
        let path = PathBuf::from("/home/user/projects/my-app");
        assert_eq!(generate_project_id(&path), "home-user-projects-my-app");
    }

    #[test]
    fn test_path_with_spaces() {
        let path = PathBuf::from(r"D:\Code\Test Project");
        assert_eq!(generate_project_id(&path), "d--code-test_project");
    }

    #[test]
    fn test_unix_root_path() {
        let path = PathBuf::from("/var/www/html");
        assert_eq!(generate_project_id(&path), "var-www-html");
    }

    #[test]
    fn test_empty_path() {
        let path = PathBuf::from("");
        assert_eq!(generate_project_id(&path), "default");
    }

    #[test]
    fn test_length_limit() {
        let long_path = "/very/long/path/".to_string() + &"segment/".repeat(20);
        let path = PathBuf::from(long_path);
        let result = generate_project_id(&path);
        assert!(result.len() <= 64);
    }
}

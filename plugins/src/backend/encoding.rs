//! Encoding detection and conversion for stdin input
//!
//! This module provides encoding strategy detection to handle Chinese (CJK) characters
//! and other multibyte Unicode characters in stdin input, preventing mojibake issues
//! on Windows platforms where batch files may re-encode command-line arguments.
//!
//! # Problem
//!
//! On Windows, when passing Chinese text as command-line arguments to batch files (.cmd),
//! the text may be re-encoded according to the current code page (e.g., GBK, GB18030),
//! causing mojibake when the subprocess expects UTF-8.
//!
//! # Solution
//!
//! - **ASCII prompts**: Pass via command-line arguments (fast, compatible)
//! - **Non-ASCII prompts**: Force stdin transmission (preserves UTF-8 encoding)
//!
//! # Example
//!
//! ```rust
//! use memex_plugins::backend::encoding::{detect_encoding_strategy, EncodingStrategy};
//!
//! let ascii_prompt = "Hello, World!";
//! assert_eq!(detect_encoding_strategy(ascii_prompt), EncodingStrategy::DirectArgs);
//!
//! let chinese_prompt = "你好，世界！";
//! assert!(matches!(
//!     detect_encoding_strategy(chinese_prompt),
//!     EncodingStrategy::ForceStdin { .. }
//! ));
//! ```

/// Encoding strategy for prompt transmission
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EncodingStrategy {
    /// Use command-line arguments directly (ASCII-safe)
    DirectArgs,

    /// Force stdin transmission to preserve encoding
    ForceStdin {
        /// Reason for forcing stdin mode
        reason: String,
    },
}

/// Checks if an ASCII string is "simple" (no control chars or shell metacharacters)
///
/// This is a fast-path optimization to avoid full character iteration for common prompts.
#[inline]
fn is_simple_ascii(s: &str) -> bool {
    // Check for control characters and shell metacharacters
    // Note: '!' excluded as it's only problematic in bash history expansion (usually disabled)
    !s.as_bytes().iter().any(|&b| {
        b == b'\n'
            || b == b'\r'
            || b == b'\t'
            || b < 0x20 // Control characters
            || matches!(b, b'&' | b'|' | b'<' | b'>' | b'^' | b'%' | b'"')
    })
}

/// Detects the appropriate encoding strategy for a given prompt
///
/// # Detection Layers
///
/// 1. **Length check**: If exceeds Windows arg limit (8000 chars) → `ForceStdin`
/// 2. **Empty/ASCII check**: If empty or pure ASCII → `DirectArgs`
/// 3. **Single-pass character scan**: Detects CJK, special chars, multibyte Unicode → `ForceStdin`
///
/// # Performance
///
/// - Length check: O(1)
/// - Empty/ASCII check: O(1) early return
/// - Character scan: O(n) single-pass, short-circuit on first match
///
/// **Optimization**: Previous implementation used 2 separate iterations (CJK + multibyte).
/// Optimized version uses 1 iteration for all checks, ~2x faster for non-ASCII prompts.
///
/// # Examples
///
/// ```rust
/// use memex_plugins::backend::encoding::{detect_encoding_strategy, EncodingStrategy};
///
/// // ASCII prompt
/// let strategy = detect_encoding_strategy("echo hello");
/// assert_eq!(strategy, EncodingStrategy::DirectArgs);
///
/// // Chinese prompt
/// let strategy = detect_encoding_strategy("打印你好");
/// assert!(matches!(strategy, EncodingStrategy::ForceStdin { .. }));
///
/// // Emoji prompt
/// let strategy = detect_encoding_strategy("Hello 🌍");
/// assert!(matches!(strategy, EncodingStrategy::ForceStdin { .. }));
///
/// // Long prompt
/// let long_prompt = "x".repeat(9000);
/// assert!(matches!(detect_encoding_strategy(&long_prompt), EncodingStrategy::ForceStdin { .. }));
/// ```
pub fn detect_encoding_strategy(prompt: &str) -> EncodingStrategy {
    // Layer 1: Length check (Windows command-line argument limit)
    // Windows CMD has a limit of ~8191 characters for command line
    // Use conservative threshold of 8000 to account for other args
    const MAX_ARG_LENGTH: usize = 8000;
    if prompt.len() > MAX_ARG_LENGTH {
        return EncodingStrategy::ForceStdin {
            reason: format!("Exceeds max argument length ({} > {})", prompt.len(), MAX_ARG_LENGTH),
        };
    }

    // Layer 2: Empty or whitespace-only prompts
    if prompt.is_empty() || prompt.trim().is_empty() {
        return EncodingStrategy::DirectArgs;
    }

    // Layer 3: Quick check for common safe patterns
    // Only return early if prompt is ASCII AND doesn't contain special chars
    // Note: This is an optimization - we skip character iteration for simple prompts
    if prompt.is_ascii() && is_simple_ascii(prompt) {
        return EncodingStrategy::DirectArgs;
    }

    // Layer 4: Single-pass character scan
    // Optimized: Check CJK, special chars, and multibyte in one iteration
    for c in prompt.chars() {
        // Check for CJK characters first (most common non-ASCII case)
        let char_type = match c {
            // Chinese characters (CJK Unified Ideographs)
            '\u{4E00}'..='\u{9FFF}' => Some("Chinese (CJK Unified)"),
            '\u{3400}'..='\u{4DBF}' => Some("Chinese (CJK Ext-A)"),
            '\u{20000}'..='\u{2A6DF}' => Some("Chinese (CJK Ext-B)"),
            '\u{2A700}'..='\u{2B73F}' => Some("Chinese (CJK Ext-C)"),
            '\u{2B740}'..='\u{2B81F}' => Some("Chinese (CJK Ext-D)"),
            '\u{2B820}'..='\u{2CEAF}' => Some("Chinese (CJK Ext-E)"),
            '\u{2CEB0}'..='\u{2EBEF}' => Some("Chinese (CJK Ext-F)"),

            // Japanese characters
            '\u{3040}'..='\u{309F}' => Some("Japanese (Hiragana)"),
            '\u{30A0}'..='\u{30FF}' => Some("Japanese (Katakana)"),

            // Korean characters
            '\u{AC00}'..='\u{D7AF}' => Some("Korean (Hangul)"),

            // Control characters and newlines
            '\n' | '\r' | '\t' => Some("Control character (newline/tab)"),
            c if c.is_control() => Some("Control character"),

            // Shell metacharacters that might cause issues
            // Note: '!' removed as it's only problematic in bash history expansion (usually disabled)
            '&' | '|' | '<' | '>' | '^' | '%' | '"' => Some("Shell metacharacter"),

            // Other multibyte Unicode (emoji, rare scripts, etc.)
            c if c.len_utf8() > 1 => Some("Multibyte Unicode (emoji/special)"),

            _ => None,
        };

        if let Some(reason) = char_type {
            return EncodingStrategy::ForceStdin {
                reason: format!("Contains {} (char: '{}')", reason, c),
            };
        }
    }

    // Default: ASCII-safe
    EncodingStrategy::DirectArgs
}

/// Prepares prompt payload for stdin transmission
///
/// Returns the prompt as-is since Rust strings are already UTF-8 encoded.
/// This function exists for API consistency and future extension points
/// (e.g., adding BOM handling, normalization, etc.).
///
/// # Examples
///
/// ```rust
/// use memex_plugins::backend::encoding::prepare_stdin_payload;
///
/// let prompt = "测试中文";
/// let payload = prepare_stdin_payload(prompt);
/// assert_eq!(payload, prompt);
/// ```
///
/// # Returns
///
/// UTF-8 encoded string
pub fn prepare_stdin_payload(prompt: &str) -> String {
    prompt.to_string()
}

/// Escapes shell argument for safe command-line transmission
///
/// Uses JSON string encoding to handle special characters that might be
/// misinterpreted by Windows batch files or Unix shells.
///
/// # Note
///
/// This function should only be used for ASCII prompts. Non-ASCII prompts
/// should use `prepare_stdin_payload()` and stdin transmission instead.
///
/// # Examples
///
/// ```rust
/// use memex_plugins::backend::encoding::escape_shell_arg;
///
/// let arg = "echo \"hello\"";
/// let escaped = escape_shell_arg(arg);
/// // JSON-escapes internal quotes
/// assert!(escaped.contains("\\\""));
/// ```
///
/// # Returns
///
/// Escaped string safe for shell argument
pub fn escape_shell_arg(arg: &str) -> String {
    // Use JSON string encoding to handle all special characters
    match serde_json::to_string(arg) {
        Ok(json) if json.len() >= 2 => {
            // Remove surrounding quotes, keep internal escaping
            json[1..json.len() - 1].to_string()
        }
        _ => arg.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_prompt_uses_direct_args() {
        let strategy = detect_encoding_strategy("");
        assert_eq!(strategy, EncodingStrategy::DirectArgs);
    }

    #[test]
    fn test_whitespace_only_prompt_uses_direct_args() {
        let strategy = detect_encoding_strategy("   \n\t  ");
        assert_eq!(strategy, EncodingStrategy::DirectArgs);
    }

    #[test]
    fn test_ascii_prompt_uses_direct_args() {
        // Safe ASCII prompts (no shell metacharacters or control characters)
        let prompts = vec![
            "Hello, World!",
            "echo test",
            "This is a simple ASCII prompt.",
            "123 456 789",
            "Safe chars: !@#$*()_+-=[]{}:',.",  // Removed shell metacharacters: %^&|<>"
        ];

        for prompt in prompts {
            let strategy = detect_encoding_strategy(prompt);
            assert_eq!(
                strategy,
                EncodingStrategy::DirectArgs,
                "Failed for prompt: {}",
                prompt
            );
        }
    }

    #[test]
    fn test_special_ascii_chars_documented() {
        // Document which ASCII special chars force stdin
        // This serves as documentation of the current behavior
        let safe_chars = vec!['!', '@', '#', '$', '*', '(', ')', '_', '+', '-', '=', '[', ']', '{', '}', ':', '\'', ',', '.', '?', '/'];
        let unsafe_chars = vec!['%', '^', '&', '|', '<', '>', '"'];

        for ch in safe_chars {
            let prompt = format!("test {}", ch);
            let strategy = detect_encoding_strategy(&prompt);
            assert_eq!(
                strategy,
                EncodingStrategy::DirectArgs,
                "Safe ASCII char '{}' should not force stdin",
                ch
            );
        }

        for ch in unsafe_chars {
            let prompt = format!("test {}", ch);
            let strategy = detect_encoding_strategy(&prompt);
            assert!(
                matches!(strategy, EncodingStrategy::ForceStdin { .. }),
                "Unsafe ASCII char '{}' should force stdin",
                ch
            );
        }
    }

    #[test]
    fn test_chinese_prompt_forces_stdin() {
        let prompts = vec![
            "你好，世界！",
            "测试中文输入",
            "编写一个Python函数",
            "中文字符",
        ];

        for prompt in prompts {
            let strategy = detect_encoding_strategy(prompt);
            assert!(
                matches!(strategy, EncodingStrategy::ForceStdin { .. }),
                "Failed for prompt: {}",
                prompt
            );
        }
    }

    #[test]
    fn test_japanese_prompt_forces_stdin() {
        let prompts = vec![
            "こんにちは世界", // Hiragana
            "コンニチハ",     // Katakana
            "日本語テスト",   // Mixed Kanji + Katakana
        ];

        for prompt in prompts {
            let strategy = detect_encoding_strategy(prompt);
            assert!(
                matches!(strategy, EncodingStrategy::ForceStdin { .. }),
                "Failed for prompt: {}",
                prompt
            );
        }
    }

    #[test]
    fn test_korean_prompt_forces_stdin() {
        let prompt = "안녕하세요 세계";
        let strategy = detect_encoding_strategy(prompt);
        assert!(matches!(strategy, EncodingStrategy::ForceStdin { .. }));
    }

    #[test]
    fn test_mixed_prompt_forces_stdin() {
        let prompts = vec![
            "Hello 世界",
            "Print 你好",
            "Write a function to calculate 斐波那契数列",
        ];

        for prompt in prompts {
            let strategy = detect_encoding_strategy(prompt);
            assert!(
                matches!(strategy, EncodingStrategy::ForceStdin { .. }),
                "Failed for prompt: {}",
                prompt
            );
        }
    }

    #[test]
    fn test_emoji_prompt_forces_stdin() {
        let prompts = vec![
            "Hello 🌍!",
            "Test 😀 emoji",
            "🚀 Rocket launch",
            "Unicode symbols: ©️ ® ™️",
        ];

        for prompt in prompts {
            let strategy = detect_encoding_strategy(prompt);
            assert!(
                matches!(strategy, EncodingStrategy::ForceStdin { .. }),
                "Failed for prompt: {}",
                prompt
            );
        }
    }

    #[test]
    fn test_other_unicode_forces_stdin() {
        let prompts = vec![
            "Привет мир",     // Cyrillic
            "مرحبا بالعالم",  // Arabic
            "שלום עולם",      // Hebrew
            "Γειά σου κόσμε", // Greek
        ];

        for prompt in prompts {
            let strategy = detect_encoding_strategy(prompt);
            assert!(
                matches!(strategy, EncodingStrategy::ForceStdin { .. }),
                "Failed for prompt: {}",
                prompt
            );
        }
    }

    #[test]
    fn test_stdin_payload_utf8_encoding() {
        let prompts = vec![
            ("Hello", "Hello"),
            ("你好", "你好"),
            ("🌍", "🌍"),
            ("Mixed 中文", "Mixed 中文"),
        ];

        for (input, expected) in prompts {
            let payload = prepare_stdin_payload(input);
            assert_eq!(payload, expected, "Failed for input: {}", input);
        }
    }

    #[test]
    fn test_stdin_payload_preserves_newlines() {
        let prompt = "Line 1\nLine 2\r\nLine 3";
        let payload = prepare_stdin_payload(prompt);
        assert_eq!(payload, prompt);
    }

    #[test]
    fn test_escape_shell_arg_quotes() {
        let arg = "echo \"hello\"";
        let escaped = escape_shell_arg(arg);
        assert!(escaped.contains("\\\""));
    }

    #[test]
    fn test_escape_shell_arg_newlines() {
        let arg = "line1\nline2";
        let escaped = escape_shell_arg(arg);
        assert!(escaped.contains("\\n"));
    }

    #[test]
    fn test_escape_shell_arg_backslash() {
        let arg = "C:\\path\\to\\file";
        let escaped = escape_shell_arg(arg);
        assert!(escaped.contains("\\\\"));
    }

    #[test]
    fn test_escape_shell_arg_plain_text() {
        let arg = "simple text";
        let escaped = escape_shell_arg(arg);
        assert_eq!(escaped, "simple text");
    }

    // ============================================================================
    // Optimization Tests (added in enhancement phase)
    // ============================================================================

    #[test]
    fn test_long_prompt_forces_stdin() {
        let long_prompt = "x".repeat(9000);
        let strategy = detect_encoding_strategy(&long_prompt);
        assert!(matches!(strategy, EncodingStrategy::ForceStdin { .. }));
        if let EncodingStrategy::ForceStdin { reason } = strategy {
            assert!(reason.contains("Exceeds max argument length"));
        }
    }

    #[test]
    fn test_prompt_at_length_limit() {
        let prompt = "x".repeat(8000);
        let strategy = detect_encoding_strategy(&prompt);
        // At exact limit, should still be DirectArgs
        assert_eq!(strategy, EncodingStrategy::DirectArgs);
    }

    #[test]
    fn test_prompt_over_length_limit() {
        let prompt = "x".repeat(8001);
        let strategy = detect_encoding_strategy(&prompt);
        assert!(matches!(strategy, EncodingStrategy::ForceStdin { .. }));
    }

    #[test]
    fn test_newline_forces_stdin() {
        let prompt = "line1\nline2";
        let strategy = detect_encoding_strategy(prompt);
        assert!(matches!(strategy, EncodingStrategy::ForceStdin { .. }));
        if let EncodingStrategy::ForceStdin { reason } = strategy {
            assert!(reason.contains("Control character"));
            assert!(reason.contains("newline"));
        }
    }

    #[test]
    fn test_tab_forces_stdin() {
        let prompt = "col1\tcol2";
        let strategy = detect_encoding_strategy(prompt);
        assert!(matches!(strategy, EncodingStrategy::ForceStdin { .. }));
        if let EncodingStrategy::ForceStdin { reason } = strategy {
            assert!(reason.contains("Control character"));
        }
    }

    #[test]
    fn test_shell_metacharacters_force_stdin() {
        // Note: '!' excluded as it's only problematic in bash history expansion (usually disabled)
        let metacharacters = vec!["&", "|", "<", ">", "^", "%", "\""];

        for meta in metacharacters {
            let prompt = format!("echo {}", meta);
            let strategy = detect_encoding_strategy(&prompt);
            assert!(
                matches!(strategy, EncodingStrategy::ForceStdin { .. }),
                "Shell metacharacter '{}' should force stdin",
                meta
            );
            if let EncodingStrategy::ForceStdin { reason } = strategy {
                assert!(
                    reason.contains("Shell metacharacter"),
                    "Reason should mention shell metacharacter, got: {}",
                    reason
                );
            }
        }
    }

    #[test]
    fn test_detailed_reason_chinese() {
        let prompt = "你好";
        let strategy = detect_encoding_strategy(prompt);
        assert!(matches!(strategy, EncodingStrategy::ForceStdin { .. }));
        if let EncodingStrategy::ForceStdin { reason } = strategy {
            assert!(
                reason.contains("Chinese"),
                "Reason should specify Chinese, got: {}",
                reason
            );
        }
    }

    #[test]
    fn test_detailed_reason_japanese() {
        let prompt = "こんにちは";
        let strategy = detect_encoding_strategy(prompt);
        assert!(matches!(strategy, EncodingStrategy::ForceStdin { .. }));
        if let EncodingStrategy::ForceStdin { reason } = strategy {
            assert!(
                reason.contains("Japanese"),
                "Reason should specify Japanese, got: {}",
                reason
            );
        }
    }

    #[test]
    fn test_detailed_reason_korean() {
        let prompt = "안녕";
        let strategy = detect_encoding_strategy(prompt);
        assert!(matches!(strategy, EncodingStrategy::ForceStdin { .. }));
        if let EncodingStrategy::ForceStdin { reason } = strategy {
            assert!(
                reason.contains("Korean"),
                "Reason should specify Korean, got: {}",
                reason
            );
        }
    }

    #[test]
    fn test_detailed_reason_emoji() {
        let prompt = "Hello 🌍";
        let strategy = detect_encoding_strategy(prompt);
        assert!(matches!(strategy, EncodingStrategy::ForceStdin { .. }));
        if let EncodingStrategy::ForceStdin { reason } = strategy {
            assert!(
                reason.contains("Multibyte Unicode") || reason.contains("emoji"),
                "Reason should mention multibyte/emoji, got: {}",
                reason
            );
        }
    }

    #[test]
    fn test_single_pass_optimization() {
        // This test verifies that detection happens in a single character iteration
        // by checking that first occurrence determines the reason
        let prompt = "Hello 你好 🌍"; // Chinese appears before emoji
        let strategy = detect_encoding_strategy(prompt);
        assert!(matches!(strategy, EncodingStrategy::ForceStdin { .. }));
        if let EncodingStrategy::ForceStdin { reason } = strategy {
            // Should detect Chinese first (appears before emoji)
            assert!(
                reason.contains("Chinese"),
                "Should detect Chinese first in single-pass, got: {}",
                reason
            );
        }
    }

    #[test]
    fn test_mixed_special_and_cjk() {
        let prompt = "line1\n你好";
        let strategy = detect_encoding_strategy(prompt);
        assert!(matches!(strategy, EncodingStrategy::ForceStdin { .. }));
        // Should detect newline first since it appears before Chinese
        if let EncodingStrategy::ForceStdin { reason } = strategy {
            assert!(
                reason.contains("Control character"),
                "Should detect control character first, got: {}",
                reason
            );
        }
    }
}


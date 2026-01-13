/// Collapse whitespace into single spaces without intermediate Vec allocation.
pub(crate) fn one_line(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut first = true;
    for word in s.split_whitespace() {
        if !first {
            result.push(' ');
        }
        result.push_str(word);
        first = false;
    }
    result
}

/// Truncate string to max_chars with "..." suffix. Optimized to avoid redundant char counting.
pub(crate) fn truncate_clean(s: &str, max_chars: usize) -> String {
    let t = s.trim().replace("\r\n", "\n");

    // Fast path: byte length <= max_chars means char count <= max_chars (UTF-8 property)
    if t.len() <= max_chars {
        return t;
    }

    // Need to count chars only when truncation may be needed
    let truncated: String = t.chars().take(max_chars).collect();

    // Check if we actually truncated anything
    if truncated.len() == t.len() {
        truncated
    } else {
        format!("{} ...", truncated)
    }
}

/// Truncate string in middle with ".." suffix. Optimized to avoid redundant char counting.
pub(crate) fn trim_mid(s: &str, max_chars: usize) -> String {
    let t = one_line(s);

    // Fast path: byte length <= max_chars means char count <= max_chars
    if t.len() <= max_chars {
        return t;
    }

    let head: String = t.chars().take(max_chars.saturating_sub(2)).collect();
    if head.len() == t.len() {
        head
    } else {
        format!("{head}..")
    }
}

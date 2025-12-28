use memex_core::error::RunnerError;

pub fn parse_env_file(path: &str) -> Result<Vec<(String, String)>, RunnerError> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| RunnerError::Spawn(format!("failed to read env file: {}", e)))?;
    let mut out = Vec::new();

    for (idx, raw_line) in content.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() {
            return Err(RunnerError::Spawn(format!(
                "env file contains empty line at {}",
                idx + 1
            )));
        }
        if line.starts_with('#') {
            continue;
        }
        let (k, v) = line.split_once('=').ok_or_else(|| {
            RunnerError::Spawn(format!(
                "invalid env line at {} (expected KEY=VALUE)",
                idx + 1
            ))
        })?;
        let key = k.trim();
        if key.is_empty() {
            return Err(RunnerError::Spawn(format!(
                "invalid env line at {} (empty key)",
                idx + 1
            )));
        }
        let value = parse_env_value(v.trim(), idx + 1)?;
        out.push((key.to_string(), value));
    }

    Ok(out)
}

fn parse_env_value(value: &str, line_no: usize) -> Result<String, RunnerError> {
    if value.len() >= 2 {
        let first = value.chars().next().unwrap();
        let last = value.chars().last().unwrap();
        if (first == '"' && last == '"') || (first == '\'' && last == '\'') {
            let inner = &value[1..value.len() - 1];
            return unescape_env_value(inner, line_no);
        }
    }
    Ok(value.to_string())
}

fn unescape_env_value(value: &str, line_no: usize) -> Result<String, RunnerError> {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        let Some(next) = chars.next() else {
            return Err(RunnerError::Spawn(format!(
                "invalid escape at line {} (trailing backslash)",
                line_no
            )));
        };
        match next {
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            't' => out.push('\t'),
            '\\' => out.push('\\'),
            '"' => out.push('"'),
            '\'' => out.push('\''),
            other => out.push(other),
        }
    }
    Ok(out)
}

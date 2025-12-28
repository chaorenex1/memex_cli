use crate::tool_event::ToolEvent;
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct ToolStep {
    pub title: String,
    pub body: String,
}

pub fn extract_tool_steps(
    events: &[ToolEvent],
    max_steps: usize,
    args_keys_max: usize,
    value_max_chars: usize,
) -> Vec<ToolStep> {
    let mut steps = Vec::new();

    // 只取最近的 tool.request，倒序扫描
    for e in events.iter().rev() {
        if steps.len() >= max_steps {
            break;
        }
        if e.event_type != "tool.request" {
            continue;
        }

        let tool = e.tool.clone().unwrap_or_else(|| "unknown".to_string());
        let action = e.action.clone().unwrap_or_else(|| "call".to_string());

        // 生成一个“稳健的摘要”（不输出全部 args）
        let args_summary = summarize_args(&e.args, args_keys_max, value_max_chars);

        steps.push(ToolStep {
            title: format!("Call tool `{}` ({})", tool, action),
            body: format!("Args summary: {}", args_summary),
        });
    }

    steps.reverse();
    steps
}

fn summarize_args(args: &Value, args_keys_max: usize, value_max_chars: usize) -> String {
    // 优先：如果有常见字段（query/path/url/code）就提取；否则列 keys
    if let Some(o) = args.as_object() {
        for k in [
            "query", "q", "path", "filepath", "file", "url", "command", "cmd", "code",
        ]
        .iter()
        {
            if let Some(v) = o.get(*k) {
                return format!("{}={}", k, shorten(v, value_max_chars));
            }
        }
        let args_keys_max = args_keys_max.max(1);
        let keys: Vec<String> = o.keys().take(args_keys_max).cloned().collect();
        return format!("keys=[{}]", keys.join(","));
    }
    "non-object args".to_string()
}

fn shorten(v: &Value, value_max_chars: usize) -> String {
    let s = match v {
        Value::String(x) => x.clone(),
        _ => v.to_string(),
    };
    let t = s.trim().replace('\n', " ");

    let value_max_chars = value_max_chars.max(1);
    if t.chars().count() <= value_max_chars {
        t
    } else {
        let take_chars = value_max_chars.saturating_sub(1).max(1);
        t.chars().take(take_chars).collect::<String>() + "…"
    }
}

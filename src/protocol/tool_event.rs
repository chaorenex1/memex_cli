use serde::Deserialize;

pub const EVENT_PREFIX: &str = "@@MEM_TOOL_EVENT@@ ";

#[derive(Debug, Deserialize)]
pub struct BaseEnvelope {
    pub v: Option<u8>,
    #[serde(rename = "type")]
    pub ty: String,
    pub id: String,
}

#[derive(Debug, Deserialize)]
pub struct ToolRequestEvent {
    pub v: Option<u8>,
    #[serde(rename = "type")]
    pub ty: String,
    pub ts: Option<String>,
    pub id: String,
    pub tool: String,
    pub action: Option<String>,
    pub args: serde_json::Value,
    pub requires_policy: Option<bool>,
    pub rationale: Option<String>,
}

#[derive(Debug)]
pub enum ToolEvent {
    Request(ToolRequestEvent),
    Other,
}

pub struct JsonlToolEventParser;

impl JsonlToolEventParser {
    pub fn parse_line(line: &str) -> Result<Option<ToolEvent>, serde_json::Error> {
        let s = if let Some(rest) = line.strip_prefix(EVENT_PREFIX) {
            rest
        } else {
            line
        };

        let env: BaseEnvelope = match serde_json::from_str(s) {
            Ok(v) => v,
            Err(_) => return Ok(None),
        };

        match env.ty.as_str() {
            "tool.request" => {
                let req: ToolRequestEvent = serde_json::from_str(s)?;
                Ok(Some(ToolEvent::Request(req)))
            }
            _ => Ok(Some(ToolEvent::Other)),
        }
    }
}

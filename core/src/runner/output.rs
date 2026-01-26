use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;

use crate::events_out::EventsOutTx;
use crate::tool_event::{
    extract_run_id_from_value, StreamJsonToolEventParser, ToolEvent, TOOL_EVENT_PREFIX,
};

use super::io_pump::{LineStream, LineTap};
use super::policy::{PolicyEngine, PolicyOutcome};
use super::RunnerEvent;

fn flow_audit_enabled() -> bool {
    std::env::var_os("MEMEX_FLOW_AUDIT")
        .map(|v| !v.is_empty() && v != "0")
        .unwrap_or(false)
}

fn audit_preview(s: &str) -> String {
    const MAX: usize = 160;
    if s.len() <= MAX {
        return s.to_string();
    }
    let end = s
        .char_indices()
        .take_while(|(i, _)| *i < MAX)
        .last()
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(0);
    let mut out = s[..end].to_string();
    out.push('…');
    out
}

#[derive(Debug, Clone)]
pub enum OutputEvent {
    RawLine { stream: LineStream, text: String },
    ToolEvent(Box<ToolEvent>),
}

#[derive(Debug, Clone)]
pub struct ParseError {
    pub stream: LineStream,
    pub line_preview: String,
    pub reason: String,
}

#[async_trait]
pub trait StreamParser: Send {
    async fn parse(&mut self, tap: &LineTap) -> Result<Vec<OutputEvent>, ParseError>;
}

pub struct JsonlParser {
    events_out: Option<EventsOutTx>,
    configured_run_id: Option<String>,
    discovered_run_id: Option<String>,
    tool_events: Vec<ToolEvent>,
    stream_json: StreamJsonToolEventParser,
    buf_out: Vec<u8>,
    buf_err: Vec<u8>,
}

impl JsonlParser {
    pub fn new(events_out: Option<EventsOutTx>, run_id: &str) -> Self {
        Self {
            events_out,
            configured_run_id: Some(run_id.to_string()),
            discovered_run_id: None,
            tool_events: Vec::new(),
            stream_json: StreamJsonToolEventParser::new(),
            buf_out: Vec::with_capacity(8 * 1024),
            buf_err: Vec::with_capacity(8 * 1024),
        }
    }

    pub fn take_tool_events(&mut self) -> Vec<ToolEvent> {
        std::mem::take(&mut self.tool_events)
    }

    pub fn dropped_events_out(&self) -> u64 {
        self.events_out
            .as_ref()
            .map(|x| x.dropped_count())
            .unwrap_or(0)
    }

    pub fn effective_run_id(&self) -> Option<&str> {
        self.discovered_run_id
            .as_deref()
            .or(self.configured_run_id.as_deref())
    }

    async fn emit_tool_event(
        events_out: &Option<EventsOutTx>,
        effective_run_id: Option<&str>,
        tool_events: &mut Vec<ToolEvent>,
        mut ev: ToolEvent,
    ) -> ToolEvent {
        if ev.run_id.is_none() {
            if let Some(id) = effective_run_id.map(|x| x.to_string()) {
                ev.run_id = Some(id);
            }
        }

        if let Some(out) = events_out {
            // Use to_writer with pre-allocated buffer to avoid intermediate allocations
            let mut buf = Vec::with_capacity(1024);
            if serde_json::to_writer(&mut buf, &ev).is_ok() {
                // SAFETY: serde_json always produces valid UTF-8
                let s = unsafe { String::from_utf8_unchecked(buf) };
                out.send_line(s).await;
            }
        }

        tool_events.push(ev.clone());
        ev
    }

    fn strip_prefix(buf: &mut Vec<u8>) {
        let prefix = TOOL_EVENT_PREFIX.as_bytes();
        if buf.starts_with(prefix) {
            buf.drain(..prefix.len());
            // Batch drain whitespace after prefix
            let skip = buf
                .iter()
                .position(|&b| !matches!(b, b' ' | b'\t'))
                .unwrap_or(buf.len());
            if skip > 0 {
                buf.drain(..skip);
            }
        }
    }

    fn strip_ws(buf: &mut Vec<u8>) {
        // Batch drain all leading whitespace in single operation
        let skip = buf
            .iter()
            .position(|&b| !matches!(b, b' ' | b'\t' | b'\r' | b'\n'))
            .unwrap_or(buf.len());
        if skip > 0 {
            buf.drain(..skip);
        }
    }

    fn drain_one_line(buf: &mut Vec<u8>) -> String {
        let end = buf
            .iter()
            .position(|&b| b == b'\n')
            .map(|i| i + 1)
            .unwrap_or(buf.len());
        let raw = buf.drain(..end).collect::<Vec<u8>>();
        String::from_utf8_lossy(&raw).trim_end().to_string()
    }

    fn try_parse_one_json(
        buf: &[u8],
    ) -> Result<Option<(serde_json::Value, usize)>, serde_json::Error> {
        let mut iter = serde_json::Deserializer::from_slice(buf).into_iter::<serde_json::Value>();
        match iter.next() {
            Some(Ok(v)) => Ok(Some((v, iter.byte_offset()))),
            Some(Err(e)) if e.is_eof() => Ok(None),
            Some(Err(e)) => Err(e),
            None => Ok(None),
        }
    }
}

#[async_trait]
impl StreamParser for JsonlParser {
    async fn parse(&mut self, tap: &LineTap) -> Result<Vec<OutputEvent>, ParseError> {
        if flow_audit_enabled() {
            tracing::debug!(
                target: "memex.flow",
                stage = "parse.jsonl.in",
                stream = ?tap.stream,
                bytes = tap.line.len(),
                preview = %audit_preview(&tap.line)
            );
        }
        let JsonlParser {
            events_out,
            configured_run_id,
            discovered_run_id,
            tool_events,
            stream_json,
            buf_out,
            buf_err,
        } = self;

        let buf: &mut Vec<u8> = match tap.stream {
            LineStream::Stdout => buf_out,
            LineStream::Stderr => buf_err,
        };

        buf.extend_from_slice(tap.line.as_bytes());
        buf.push(b'\n');

        let mut out: Vec<OutputEvent> = Vec::new();

        loop {
            Self::strip_ws(buf);
            if buf.is_empty() {
                break;
            }

            Self::strip_prefix(buf);
            Self::strip_ws(buf);
            if buf.is_empty() {
                break;
            }

            if !matches!(buf.first(), Some(b'{' | b'[')) {
                let line = Self::drain_one_line(buf);
                return Err(ParseError {
                    stream: tap.stream,
                    line_preview: truncate(&line, 240),
                    reason: "non_json_line".to_string(),
                });
            }

            let parsed = match Self::try_parse_one_json(buf) {
                Ok(Some((v, consumed))) => (v, consumed),
                Ok(None) => break, // need more data
                Err(e) => {
                    return Err(ParseError {
                        stream: tap.stream,
                        line_preview: truncate(&String::from_utf8_lossy(buf), 240),
                        reason: format!("invalid_json: {}", e),
                    });
                }
            };

            let (value, consumed) = parsed;
            buf.drain(..consumed);

            if discovered_run_id.is_none() {
                if let Some(id) = extract_run_id_from_value(&value) {
                    *discovered_run_id = Some(id);
                    if flow_audit_enabled() {
                        tracing::debug!(
                            target: "memex.flow",
                            stage = "parse.jsonl.run_id",
                            run_id = %discovered_run_id.as_deref().unwrap_or("")
                        );
                    }
                }
            }

            // Try stream_json parser first (takes reference, no clone needed)
            // Fall back to direct deserialization only if stream_json doesn't match
            let ev = stream_json
                .parse_value(&value)
                .or_else(|| serde_json::from_value::<ToolEvent>(value).ok());

            match ev {
                Some(ev) => {
                    let effective = discovered_run_id
                        .as_deref()
                        .or(configured_run_id.as_deref());
                    let ev = Self::emit_tool_event(events_out, effective, tool_events, ev).await;
                    if flow_audit_enabled() {
                        tracing::debug!(
                            target: "memex.flow",
                            stage = "parse.jsonl.out",
                            event_type = %ev.event_type
                        );
                    }
                    out.push(OutputEvent::ToolEvent(Box::new(ev)));
                }
                None => {
                    // Not a ToolEvent or known stream-json shape. Skip silently.
                }
            }
        }

        if flow_audit_enabled() {
            tracing::debug!(
                target: "memex.flow",
                stage = "parse.jsonl.done",
                produced = out.len()
            );
        }
        Ok(out)
    }
}

pub struct TextParser {
    jsonl: JsonlParser,
}

impl TextParser {
    pub fn new(events_out: Option<EventsOutTx>, run_id: &str) -> Self {
        Self {
            jsonl: JsonlParser::new(events_out, run_id),
        }
    }
}

#[async_trait]
impl StreamParser for TextParser {
    async fn parse(&mut self, tap: &LineTap) -> Result<Vec<OutputEvent>, ParseError> {
        if flow_audit_enabled() {
            tracing::debug!(
                target: "memex.flow",
                stage = "parse.text.in",
                stream = ?tap.stream,
                bytes = tap.line.len(),
                preview = %audit_preview(&tap.line)
            );
        }

        match self.jsonl.parse(tap).await {
            Ok(events) => {
                // If we extracted ToolEvents, pass them through.
                Ok(events
                    .iter()
                    .map(|e| OutputEvent::RawLine {
                        stream: tap.stream,
                        text: match e {
                            OutputEvent::RawLine { text, .. } => text.clone(),
                            OutputEvent::ToolEvent(te) => te
                                .output
                                .as_ref()
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                        },
                    })
                    .collect())
            }
            Err(e) => {
                // Parsing failed; fall through to raw line output.
                // If the line looks like a tool event (prefixed or JSON), report parse error.
                Err(ParseError {
                    stream: tap.stream,
                    line_preview: truncate(&tap.line, 240),
                    reason: format!("invalid_tool_event_line: {}", e.reason),
                })
            }
        }
    }
}

#[async_trait]
pub trait OutputSink: Send {
    async fn emit(&mut self, ev: OutputEvent);
}

pub struct HttpSseSink {
    tx: mpsc::UnboundedSender<Vec<u8>>,
}

impl HttpSseSink {
    pub fn new(tx: mpsc::UnboundedSender<Vec<u8>>) -> Self {
        Self { tx }
    }

    fn format_sse(event: &str, data: &str) -> Vec<u8> {
        // Server-Sent Events framing:
        // event: <name>\n
        // data: <line>\n
        // ...\n
        // \n
        let mut out = String::new();
        out.push_str("event: ");
        out.push_str(event);
        out.push('\n');
        for line in data.split('\n') {
            out.push_str("data: ");
            out.push_str(line);
            out.push('\n');
        }
        out.push('\n');
        out.into_bytes()
    }

    fn send(&self, event: &str, data: &str) {
        let _ = self.tx.send(Self::format_sse(event, data));
    }
}

#[async_trait]
impl OutputSink for HttpSseSink {
    async fn emit(&mut self, ev: OutputEvent) {
        match ev {
            OutputEvent::RawLine { stream, text } => {
                let event = match stream {
                    LineStream::Stdout => "stdout",
                    LineStream::Stderr => "stderr",
                };
                self.send(event, &text);
            }
            OutputEvent::ToolEvent(tool_ev) => {
                // Stream structured events as JSON payload.
                let event = tool_ev.event_type.as_str();
                let mut buf = Vec::with_capacity(1024);
                let data = if serde_json::to_writer(&mut buf, tool_ev.as_ref()).is_ok() {
                    // SAFETY: serde_json always produces valid UTF-8
                    unsafe { String::from_utf8_unchecked(buf) }
                } else {
                    "{}".to_string()
                };
                self.send(event, &data);
            }
        }
    }
}

pub struct TuiSink {
    tx: tokio::sync::mpsc::UnboundedSender<RunnerEvent>,
}

impl TuiSink {
    pub fn new(tx: tokio::sync::mpsc::UnboundedSender<RunnerEvent>) -> Self {
        Self { tx }
    }

    pub fn send_error(&self, msg: String) {
        let _ = self.tx.send(RunnerEvent::Error(msg));
    }

    pub fn send_run_complete(&self, exit_code: i32) {
        let _ = self.tx.send(RunnerEvent::RunComplete { exit_code });
    }
}

#[async_trait]
impl OutputSink for TuiSink {
    async fn emit(&mut self, ev: OutputEvent) {
        if flow_audit_enabled() {
            match &ev {
                OutputEvent::RawLine { stream, text } => tracing::debug!(
                    target: "memex.flow",
                    stage = "sink.tui.in",
                    kind = "raw_line",
                    stream = ?stream,
                    bytes = text.len(),
                    preview = %audit_preview(text)
                ),
                OutputEvent::ToolEvent(tool_ev) => tracing::debug!(
                    target: "memex.flow",
                    stage = "sink.tui.in",
                    kind = "tool_event",
                    event_type = %tool_ev.event_type
                ),
            }
        }
        match ev {
            OutputEvent::ToolEvent(tool_ev) => {
                if tool_ev.event_type == "assistant.output" {
                    let text = tool_ev
                        .output
                        .as_ref()
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    if !text.is_empty() {
                        let _ = self.tx.send(RunnerEvent::AssistantOutput(text));
                    }
                } else {
                    let _ = self.tx.send(RunnerEvent::ToolEvent(tool_ev));
                }
            }
            OutputEvent::RawLine { stream, text } => match stream {
                LineStream::Stdout => {
                    let _ = self.tx.send(RunnerEvent::RawStdout(text.clone()));
                    let _ = self.tx.send(RunnerEvent::AssistantOutput(text));
                }
                LineStream::Stderr => {
                    let _ = self.tx.send(RunnerEvent::RawStderr(text));
                }
            },
        }
    }
}

pub struct StdioSink {
    stdout: tokio::io::Stdout,
    stderr: tokio::io::Stderr,
}

impl StdioSink {
    pub fn new() -> Self {
        Self {
            stdout: tokio::io::stdout(),
            stderr: tokio::io::stderr(),
        }
    }

    fn audit_preview(s: &str) -> String {
        // Keep audit logs compact and safe for stderr.
        const MAX: usize = 120;
        if s.len() <= MAX {
            return s.to_string();
        }
        // 找到不超过 MAX 的最近字符边界
        let end = s
            .char_indices()
            .take_while(|(i, _)| *i < MAX)
            .last()
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(0);
        let mut out = s[..end].to_string();
        out.push('…');
        out
    }

    async fn write_line(writer: &mut (dyn tokio::io::AsyncWrite + Unpin + Send), s: &str) {
        let _ = writer.write_all(s.as_bytes()).await;
        let _ = writer.write_all(b"\n").await;
        let _ = writer.flush().await;
    }
}

#[async_trait]
impl OutputSink for StdioSink {
    async fn emit(&mut self, ev: OutputEvent) {
        if flow_audit_enabled() {
            match &ev {
                OutputEvent::RawLine { stream, text } => tracing::debug!(
                    target: "memex.flow",
                    stage = "sink.stdio.in",
                    kind = "raw_line",
                    stream = ?stream,
                    bytes = text.len(),
                    preview = %audit_preview(text)
                ),
                OutputEvent::ToolEvent(tool_ev) => tracing::debug!(
                    target: "memex.flow",
                    stage = "sink.stdio.in",
                    kind = "tool_event",
                    event_type = %tool_ev.event_type
                ),
            }
        }
        match ev {
            OutputEvent::RawLine { stream, text } => match stream {
                LineStream::Stdout => {
                    tracing::debug!(
                        target: "memex.stdout_audit",
                        kind = "raw_line",
                        bytes = text.len(),
                        preview = %Self::audit_preview(&text)
                    );
                    Self::write_line(&mut self.stdout, &text).await
                }
                LineStream::Stderr => {
                    Self::write_line(&mut self.stderr, &Self::audit_preview(&text)).await
                }
            },
            OutputEvent::ToolEvent(ev) => {
                // Use to_writer with pre-allocated buffer for better performance
                let mut buf = Vec::with_capacity(1024);
                let s = if serde_json::to_writer(&mut buf, ev.as_ref()).is_ok() {
                    // SAFETY: serde_json always produces valid UTF-8
                    unsafe { String::from_utf8_unchecked(buf) }
                } else {
                    "{}".to_string()
                };
                tracing::debug!(
                    target: "memex.stdout_audit",
                    kind = "tool_event",
                    bytes = s.len(),
                    preview = %Self::audit_preview(&s)
                );
                Self::write_line(&mut self.stdout, &s).await
            }
        }
    }
}

pub async fn maybe_apply_policy(
    backend_kind: &str,
    policy_engine: &mut PolicyEngine,
    policy: Option<&dyn super::traits::PolicyPlugin>,
    ctl_tx: &tokio::sync::mpsc::Sender<serde_json::Value>,
    run_id: &str,
    ev: &ToolEvent,
) -> PolicyOutcome {
    if backend_kind == "codecli" {
        return PolicyOutcome::Continue;
    }
    if ev.event_type != "tool.request" {
        return PolicyOutcome::Continue;
    }
    policy_engine
        .on_tool_request(ev, policy, ctl_tx, run_id)
        .await
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let end = s
        .char_indices()
        .take_while(|(i, _)| *i < max)
        .last()
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(0);
    let mut out = s[..end].to_string();
    out.push('…');
    out
}

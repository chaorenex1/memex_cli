use crate::events_out::EventsOutTx;
use crate::tool_event::{extract_run_id_from_line, ToolEvent, ToolEventParser};

pub struct ToolEventRuntime<P: ToolEventParser> {
    parser: P,
    events: Vec<ToolEvent>,
    events_out: Option<EventsOutTx>,
    configured_run_id: Option<String>,
    discovered_run_id: Option<String>,
}

impl<P: ToolEventParser> ToolEventRuntime<P> {
    pub fn new(parser: P, events_out: Option<EventsOutTx>, run_id: Option<String>) -> Self {
        Self {
            parser,
            events: Vec::new(),
            events_out,
            configured_run_id: run_id,
            discovered_run_id: None,
        }
    }

    pub async fn send_out(&self, mut ev: ToolEvent) {
        if ev.run_id.is_none() {
            if let Some(id) = self.effective_run_id().map(|x| x.to_string()) {
                ev.run_id = Some(id);
            }
        }

        if let Some(out) = &self.events_out {
            // Use to_writer with pre-allocated buffer for better performance
            let mut buf = Vec::with_capacity(512);
            if serde_json::to_writer(&mut buf, &ev).is_ok() {
                // SAFETY: serde_json always produces valid UTF-8
                let s = unsafe { String::from_utf8_unchecked(buf) };
                out.send_line(s).await;
            }
        }
    }

    pub async fn observe_line(&mut self, line: &str) -> Option<ToolEvent> {
        if self.discovered_run_id.is_none() {
            if let Some(id) = extract_run_id_from_line(line) {
                self.discovered_run_id = Some(id);
            }
        }

        if let Some(ev) = self.parser.parse_line(line) {
            let mut ev = ev;

            if ev.run_id.is_none() {
                if let Some(id) = self.effective_run_id().map(|x| x.to_string()) {
                    ev.run_id = Some(id);
                }
            }

            if let Some(out) = &self.events_out {
                // Use to_writer with pre-allocated buffer for better performance
                let mut buf = Vec::with_capacity(512);
                if serde_json::to_writer(&mut buf, &ev).is_ok() {
                    // SAFETY: serde_json always produces valid UTF-8
                    let s = unsafe { String::from_utf8_unchecked(buf) };
                    out.send_line(s).await;
                }
            }

            self.events.push(ev.clone());
            return Some(ev);
        }
        None
    }

    pub fn effective_run_id(&self) -> Option<&str> {
        self.discovered_run_id
            .as_deref()
            .or(self.configured_run_id.as_deref())
    }

    pub fn take_events(&mut self) -> Vec<ToolEvent> {
        std::mem::take(&mut self.events)
    }

    pub fn dropped_events_out(&self) -> u64 {
        self.events_out
            .as_ref()
            .map(|x| x.dropped_count())
            .unwrap_or(0)
    }
}

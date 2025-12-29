use crate::events_out::EventsOutTx;
use crate::tool_event::{
    CompositeToolEventParser, PrefixedJsonlParser, ToolEvent, ToolEventRuntime, TOOL_EVENT_PREFIX,
};

pub struct ToolObserver {
    runtime: ToolObserverRuntime,
}

enum ToolObserverRuntime {
    Composite(ToolEventRuntime<CompositeToolEventParser>),
    PrefixedOnly(ToolEventRuntime<PrefixedJsonlParser>),
}

impl ToolObserver {
    pub fn new(events_out: Option<EventsOutTx>, run_id: &str, stream_format: &str) -> Self {
        let runtime = if stream_format == "jsonl" {
            let parser = CompositeToolEventParser::new(TOOL_EVENT_PREFIX);
            ToolObserverRuntime::Composite(ToolEventRuntime::new(
                parser,
                events_out,
                Some(run_id.to_string()),
            ))
        } else {
            // text mode: only recognize stable prefixed ToolEvent lines, do not interpret
            // external stream-json blobs as tool events.
            let parser = PrefixedJsonlParser::new(TOOL_EVENT_PREFIX);
            ToolObserverRuntime::PrefixedOnly(ToolEventRuntime::new(
                parser,
                events_out,
                Some(run_id.to_string()),
            ))
        };

        Self { runtime }
    }

    pub async fn observe_line(&mut self, line: &str) -> Option<ToolEvent> {
        match &mut self.runtime {
            ToolObserverRuntime::Composite(r) => r.observe_line(line).await,
            ToolObserverRuntime::PrefixedOnly(r) => r.observe_line(line).await,
        }
    }

    pub async fn send_out(&self, ev: ToolEvent) {
        match &self.runtime {
            ToolObserverRuntime::Composite(r) => r.send_out(ev).await,
            ToolObserverRuntime::PrefixedOnly(r) => r.send_out(ev).await,
        }
    }

    pub fn take_events(&mut self) -> Vec<ToolEvent> {
        match &mut self.runtime {
            ToolObserverRuntime::Composite(r) => r.take_events(),
            ToolObserverRuntime::PrefixedOnly(r) => r.take_events(),
        }
    }

    pub fn dropped_events_out(&self) -> u64 {
        match &self.runtime {
            ToolObserverRuntime::Composite(r) => r.dropped_events_out(),
            ToolObserverRuntime::PrefixedOnly(r) => r.dropped_events_out(),
        }
    }

    pub fn effective_run_id(&self) -> Option<&str> {
        match &self.runtime {
            ToolObserverRuntime::Composite(r) => r.effective_run_id(),
            ToolObserverRuntime::PrefixedOnly(r) => r.effective_run_id(),
        }
    }
}

use crate::events_out::EventsOutTx;
use crate::tool_event::{CompositeToolEventParser, ToolEvent, ToolEventRuntime, TOOL_EVENT_PREFIX};

pub struct ToolObserver {
    runtime: ToolEventRuntime<CompositeToolEventParser>,
}

impl ToolObserver {
    pub fn new(events_out: Option<EventsOutTx>, run_id: &str) -> Self {
        let parser = CompositeToolEventParser::new(TOOL_EVENT_PREFIX);
        let runtime = ToolEventRuntime::new(parser, events_out, Some(run_id.to_string()));
        Self { runtime }
    }

    pub async fn observe_line(&mut self, line: &str) -> Option<ToolEvent> {
        self.runtime.observe_line(line).await
    }

    pub fn take_events(&mut self) -> Vec<ToolEvent> {
        self.runtime.take_events()
    }

    pub fn dropped_events_out(&self) -> u64 {
        self.runtime.dropped_events_out()
    }

    pub fn effective_run_id(&self) -> Option<&str> {
        self.runtime.effective_run_id()
    }
}

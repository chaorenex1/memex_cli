use crate::events_out::EventsOutTx;
use crate::tool_event::WrapperEvent;

pub async fn write_wrapper_event(out: Option<&EventsOutTx>, ev: &WrapperEvent) {
    let Some(out) = out else {
        return;
    };
    if let Ok(line) = serde_json::to_string(ev) {
        out.send_line(line).await;
    }
}

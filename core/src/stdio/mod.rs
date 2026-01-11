pub mod files;
mod id_gen;
pub mod metrics;
mod parser;
pub mod parsers;
pub mod protocol;
mod render;
mod retry;
mod types;

pub use crate::error::stdio::{ErrorCode, StdioError, StdioParseError};
#[allow(deprecated)]
pub use files::{compose_prompt, resolve_files};
pub use id_gen::generate_task_id;
pub use parser::parse_stdio_tasks;
pub use parsers::StandardStdioParser;
pub use protocol::{FormatError, FormatValidation, FormatWarning, StdioProtocolParser};
pub use render::{
    configure_event_buffer, emit_json, flush_event_buffer, render_task_jsonl, render_task_stream,
    JsonlEvent, RenderOutcome, RenderTaskInfo, TextMarkers,
};
pub use retry::{effective_timeout_secs, exit_code_for_timeout, max_attempts};
pub use types::{FilesEncoding, FilesMode, StdioRunOpts, StdioTask};

pub mod files;
mod id_gen;
pub mod metrics;
mod parser;
pub mod parsers;
pub mod protocol;
mod render;
mod retry;
pub mod serde_utils;
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
pub use serde_utils::{
    read_stdio_run_opts_json_file, read_stdio_task_json_file, read_stdio_tasks_json_file,
    stdio_run_opts_from_json, stdio_run_opts_to_json, stdio_run_opts_to_pretty_json,
    stdio_task_from_json, stdio_task_to_json, stdio_task_to_pretty_json, stdio_tasks_from_json,
    stdio_tasks_to_json, write_stdio_run_opts_json_file, write_stdio_task_json_file,
    write_stdio_tasks_json_file,
};
pub use types::{FilesEncoding, FilesMode, StdioRunOpts, StdioTask};

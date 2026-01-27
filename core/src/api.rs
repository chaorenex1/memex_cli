//! Stable re-exports for consumers (`cli`, `plugins`, and external crates).
//!
//! Prefer importing from `memex_core::api` instead of reaching into internal modules.

pub use crate::backend::{BackendPlan, BackendPlanRequest, BackendStrategy};
pub use crate::config::{
    get_memex_data_dir, load_default, AppConfig, BackendKind, ConflictResolution, ControlConfig,
    EmbeddingProvider, GatekeeperProvider, HttpServerConfig, LoggingConfig, MemoryProvider,
    PolicyConfig, PolicyProvider, PolicyRule, PromptInjectPlacement, RunnerConfig, SyncStrategy,
    TuiConfig,
};
pub use crate::context::{AppContext, Services, ServicesFactory};
pub use crate::engine::{
    post_run, pre_run, run_with_query, PreRun, RunSessionInput, RunWithQueryArgs, RunnerSpec,
};
pub use crate::error::{CliError, ExecutorError, RunnerError};
pub use crate::events_out::EventsOutTx;
pub use crate::executor::types::{
    ConcurrencyConfig, ExecutionConfig, FileProcessingConfig, OutputConfig, RetryConfig,
};
pub use crate::executor::{
    emit_debug, emit_info, emit_run_end, emit_run_start, emit_warning, execute_tasks,
    ExecutionEngine, ExecutionOpts, ExecutionResult, ProgressMonitor, TaskGraph, TaskResult,
};
pub use crate::gatekeeper::evaluate::prepare_inject_list;
pub use crate::gatekeeper::{
    Gatekeeper, GatekeeperConfig, GatekeeperDecision, GatekeeperPlugin, InjectItem, SearchMatch,
    TaskGradeResult,
};
pub use crate::input::InputParser;
pub use crate::memory::{
    build_candidate_payloads, build_hit_payload, build_validate_payloads, extract_candidates,
    parse_search_matches, CandidateDraft, CandidateExtractConfig, MemoryPlugin, QACandidatePayload,
    QAHitsPayload, QAReferencePayload, QASearchPayload, QAValidationPayload, SyncStatusReport,
    SyncableMemory,
};
pub use crate::replay::{replay_cmd, ReplayArgs};
pub use crate::runner::{
    run_session, ParserKind, PolicyAction, PolicyPlugin, RunOutcome, RunSessionArgs, RunnerEvent,
    RunnerPlugin, RunnerResult, RunnerSession, RunnerStartArgs, Signal, SinkKind,
};

pub use crate::stdio::{
    compose_prompt, configure_event_buffer, emit_json as emit_stdio_json, flush_event_buffer,
    parse_stdio_tasks, read_stdio_run_opts_json_file, read_stdio_task_json_file,
    read_stdio_tasks_json_file, render_task_jsonl, render_task_stream, resolve_files,
    stdio_run_opts_from_json, stdio_run_opts_to_json, stdio_run_opts_to_pretty_json,
    stdio_task_from_json, stdio_task_to_json, stdio_task_to_pretty_json, stdio_tasks_from_json,
    stdio_tasks_to_json, write_stdio_run_opts_json_file, write_stdio_task_json_file,
    write_stdio_tasks_json_file, ErrorCode, FilesEncoding, FilesMode, FormatError,
    FormatValidation, FormatWarning, JsonlEvent, RenderOutcome, RenderTaskInfo,
    StandardStdioParser, StdioError, StdioParseError, StdioProtocolParser, StdioRunOpts, StdioTask,
    TextMarkers,
};
pub use crate::tool_event::{
    CompositeToolEventParser, MultiToolEventLineParser, StreamJsonToolEventParser, ToolEvent,
    ToolEventLite, ToolEventRuntime, WrapperEvent, TOOL_EVENT_PREFIX,
};

pub use crate::util::generate_project_id;

//! Stable re-exports for consumers (`cli`, `plugins`, and external crates).
//!
//! Prefer importing from `memex_core::api` instead of reaching into internal modules.

pub use crate::backend::{BackendPlan, BackendPlanRequest, BackendStrategy};
pub use crate::config::{
    load_default, AppConfig, BackendKind, ControlConfig, GatekeeperProvider, HttpServerConfig,
    LoggingConfig, MemoryProvider, PolicyConfig, PolicyProvider, PolicyRule,
    PromptInjectPlacement, RunnerConfig, TuiConfig,
};
pub use crate::context::{AppContext, Services, ServicesFactory};
pub use crate::engine::{run_with_query, RunSessionInput, RunWithQueryArgs, RunnerSpec};
pub use crate::error::{CliError, RunnerError};
pub use crate::events_out::EventsOutTx;
pub use crate::gatekeeper::evaluate::prepare_inject_list;
pub use crate::gatekeeper::{
    Gatekeeper, GatekeeperConfig, GatekeeperDecision, GatekeeperPlugin, InjectItem, SearchMatch,
    TaskGradeResult,
};
pub use crate::memory::{
    parse_search_matches, MemoryPlugin, QACandidatePayload, QAHitsPayload, QAReferencePayload,
    QASearchPayload, QAValidationPayload,
};
pub use crate::replay::{replay_cmd, ReplayArgs};
pub use crate::runner::{
    run_session, PolicyAction, PolicyPlugin, RunOutcome, RunSessionArgs, RunnerEvent, RunnerPlugin,
    RunnerResult, RunnerSession, RunnerStartArgs, Signal,
};
pub use crate::stdio::{
    emit_json as emit_stdio_json, parse_stdio_tasks, render_task_jsonl, render_task_stream,
    run_stdio, ErrorCode, FilesEncoding, FilesMode, JsonlEvent, RenderOutcome, RenderTaskInfo,
    StdioError, StdioParseError, StdioRunOpts, StdioTask, TextMarkers,
};
pub use crate::tool_event::{
    CompositeToolEventParser, MultiToolEventLineParser, ToolEvent, ToolEventRuntime, WrapperEvent,
    TOOL_EVENT_PREFIX,
};

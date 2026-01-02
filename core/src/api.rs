//! Stable re-exports for consumers (`cli`, `plugins`, and external crates).
//!
//! Prefer importing from `memex_core::api` instead of reaching into internal modules.

pub use crate::backend::{BackendPlan, BackendStrategy};
pub use crate::config::{
    load_default, AppConfig, ControlConfig, GatekeeperProvider, LoggingConfig, MemoryProvider,
    PolicyConfig, PolicyProvider, PolicyRule, PromptInjectPlacement, RunnerConfig, TuiConfig,
};
pub use crate::context::{AppContext, Services, ServicesFactory};
pub use crate::engine::{run_with_query, RunSessionInput, RunWithQueryArgs, RunnerSpec};
pub use crate::error::{CliError, RunnerError};
pub use crate::events_out::EventsOutTx;
pub use crate::gatekeeper::{
    Gatekeeper, GatekeeperConfig, GatekeeperDecision, GatekeeperPlugin, SearchMatch,
    TaskGradeResult,
};
pub use crate::memory::{
    parse_search_matches, MemoryClient, MemoryPlugin, QACandidatePayload, QAHitsPayload,
    QASearchPayload, QAValidationPayload,
};
pub use crate::replay::{replay_cmd, ReplayArgs};
pub use crate::runner::{
    run_session, PolicyAction, PolicyPlugin, RunOutcome, RunSessionArgs, RunnerEvent, RunnerPlugin,
    RunnerResult, RunnerSession, RunnerStartArgs, Signal,
};
pub use crate::tool_event::{
    CompositeToolEventParser, MultiToolEventLineParser, ToolEvent, ToolEventRuntime, WrapperEvent,
    TOOL_EVENT_PREFIX,
};

use chrono::{DateTime, Utc};
use memex_core::gatekeeper::{
    Gatekeeper, GatekeeperConfig, GatekeeperDecision, GatekeeperPlugin, SearchMatch,
};
use memex_core::runner::RunOutcome;
use memex_core::tool_event::ToolEvent;

pub struct StandardGatekeeperPlugin {
    config: GatekeeperConfig,
}

impl StandardGatekeeperPlugin {
    pub fn new(config: GatekeeperConfig) -> Self {
        Self { config }
    }
}

impl GatekeeperPlugin for StandardGatekeeperPlugin {
    fn name(&self) -> &str {
        "standard"
    }

    fn evaluate(
        &self,
        now: DateTime<Utc>,
        matches: &[SearchMatch],
        outcome: &RunOutcome,
        events: &[ToolEvent],
    ) -> GatekeeperDecision {
        // Delegate to existing logic in src/gatekeeper/evaluate.rs
        // We might want to move that logic here eventually, but for now delegating is safer.
        Gatekeeper::evaluate(&self.config, now, matches, outcome, events)
    }
}

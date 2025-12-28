use async_trait::async_trait;
use memex_core::config::{PolicyConfig, PolicyProvider};
use memex_core::runner::{PolicyAction, PolicyPlugin};
use memex_core::tool_event::ToolEvent;

pub struct ConfigPolicyPlugin {
    config: PolicyConfig,
}

impl ConfigPolicyPlugin {
    pub fn new(config: PolicyConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl PolicyPlugin for ConfigPolicyPlugin {
    fn name(&self) -> &str {
        "config"
    }

    async fn check(&self, event: &ToolEvent) -> PolicyAction {
        let PolicyProvider::Config(inner_cfg) = &self.config.provider;

        let tool_name = event.tool.as_deref().unwrap_or("unknown");
        let action_name = event.action.as_deref();

        // 1. Check denylist
        for rule in &inner_cfg.denylist {
            if rule_matches(rule, tool_name, action_name) {
                return PolicyAction::Deny {
                    reason: rule
                        .reason
                        .clone()
                        .unwrap_or_else(|| "Denied by rule".into()),
                };
            }
        }

        // 2. Check allowlist
        for rule in &inner_cfg.allowlist {
            if rule_matches(rule, tool_name, action_name) {
                return PolicyAction::Allow;
            }
        }

        // 3. Default action
        match inner_cfg.default_action.as_str() {
            "allow" => PolicyAction::Allow,
            "ask" => PolicyAction::Ask {
                prompt: format!("Allow tool {}?", tool_name),
            },
            _ => PolicyAction::Deny {
                reason: "Default deny".into(),
            },
        }
    }
}

fn rule_matches(rule: &memex_core::config::PolicyRule, tool: &str, action: Option<&str>) -> bool {
    // Simple wildcard matching for now
    if rule.tool == "*" || rule.tool == tool {
        if let Some(rule_action) = &rule.action {
            if let Some(act) = action {
                return rule_action == "*" || rule_action == act;
            }
            return false; // Rule specifies action but event has none
        }
        return true; // Rule matches tool, no action specified (matches all)
    }

    // Handle "git.*" style
    if rule.tool.ends_with(".*") {
        let prefix = &rule.tool[..rule.tool.len() - 2];
        if tool.starts_with(prefix) {
            // We don't check action if tool matches wildcard prefix?
            // Logic depends on requirement. Assuming yes for now.
            return true;
        }
    }

    false
}

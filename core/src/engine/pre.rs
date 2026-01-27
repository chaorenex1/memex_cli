//! 引擎 pre-run：可选记忆检索与 prompt 注入，产出合并后的 query 与 wrapper 事件（用于 replay/观测）。
use crate::context::Services;
use crate::gatekeeper::{GatekeeperPlugin, SearchMatch};
use crate::memory::{
    merge_prompt, render_memory_context, InjectConfig, InjectPlacement, MemoryPlugin,
    QASearchPayload,
};
use crate::tool_event::WrapperEvent;

pub(crate) struct EngineContext<'a> {
    pub project_id: &'a str,
    pub inject_cfg: &'a InjectConfig,
    pub memory: Option<&'a dyn MemoryPlugin>,
    pub gatekeeper: &'a dyn GatekeeperPlugin,
    pub memory_search_limit: u32,
    pub memory_min_score: f32,
}

pub struct PreRun {
    pub merged_query: String,
    pub shown_qa_ids: Vec<String>,
    pub matches: Vec<SearchMatch>,
    pub memory_search_event: Option<WrapperEvent>,
}

pub async fn pre_run(
    project_id: &str,
    cfg: &crate::config::AppConfig,
    services: &Services,
    user_query: &str,
) -> PreRun {
    let (memory_search_limit, memory_min_score) = match &cfg.memory.provider {
        crate::config::MemoryProvider::Service(svc_cfg) => {
            (svc_cfg.search_limit, svc_cfg.min_score)
        }
        crate::config::MemoryProvider::Local(local_cfg) => {
            (local_cfg.search_limit, local_cfg.min_score)
        }
        crate::config::MemoryProvider::Hybrid(hybrid_cfg) => {
            (hybrid_cfg.local.search_limit, hybrid_cfg.local.min_score)
        }
    };

    let inject_cfg: InjectConfig = InjectConfig {
        placement: match cfg.prompt_inject.placement {
            crate::config::PromptInjectPlacement::System => InjectPlacement::System,
            crate::config::PromptInjectPlacement::User => InjectPlacement::User,
        },
        max_items: cfg.prompt_inject.max_items,
        max_answer_chars: cfg.prompt_inject.max_answer_chars,
        include_meta_line: cfg.prompt_inject.include_meta_line,
    };

    let ctx = EngineContext {
        project_id,
        inject_cfg: &inject_cfg,
        memory: services.memory.as_deref(),
        gatekeeper: services.gatekeeper.as_ref(),
        memory_search_limit,
        memory_min_score,
    };

    tracing::info!(
        target: "memex.qa",
        stage = "pre.start",
        project_id = %ctx.project_id,
        query_len = user_query.len(),
        memory_enabled = ctx.memory.is_some(),
        limit = ctx.memory_search_limit,
        min_score = ctx.memory_min_score
    );
    let Some(mem) = ctx.memory else {
        return PreRun {
            merged_query: user_query.to_string(),
            shown_qa_ids: vec![],
            matches: vec![],
            memory_search_event: None,
        };
    };

    let payload = QASearchPayload {
        project_id: ctx.project_id.to_string(),
        query: user_query.to_string(),
        limit: ctx.memory_search_limit,
        min_score: ctx.memory_min_score,
    };

    tracing::info!(target: "memex.qa", stage = "memory.search.in");
    let matches = match mem.search(payload).await {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!("memory search failed: {}", e);
            tracing::debug!(target: "memex.qa", stage = "memory.search.out", ok = false);
            return PreRun {
                merged_query: user_query.to_string(),
                shown_qa_ids: vec![],
                matches: vec![],
                memory_search_event: None,
            };
        }
    };
    tracing::info!(
        target: "memex.qa",
        stage = "memory.search.out",
        ok = true,
        matches = matches.len()
    );

    let mut ev = WrapperEvent::new("memory.search.result", chrono::Local::now().to_rfc3339());
    ev.data = Some(serde_json::json!({
        "query": user_query,
        "matches": matches.clone(),
    }));

    let inject_list = ctx.gatekeeper.prepare_inject(&matches);

    tracing::info!(
        target: "memex.qa",
        stage = "gatekeeper.inject",
        inject_count = inject_list.len()
    );

    let memory_ctx = render_memory_context(&inject_list, ctx.inject_cfg);
    let merged = merge_prompt(user_query, &memory_ctx);
    let shown: Vec<String> = inject_list.iter().map(|x| x.qa_id.clone()).collect();

    tracing::info!(
        target: "memex.qa",
        stage = "pre.end",
        merged_query_len = merged.len(),
        shown = shown.len()
    );
    PreRun {
        merged_query: merged,
        shown_qa_ids: shown,
        matches,
        memory_search_event: Some(ev),
    }
}

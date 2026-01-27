use crate::config::AppConfig;
use crate::error::RunnerError;
use crate::events_out::{start_events_out, EventsOutTx};
use crate::gatekeeper::GatekeeperPlugin;
use crate::memory::MemoryPlugin;
use crate::runner::PolicyPlugin;
use std::sync::Arc;

#[derive(Clone)]
pub struct Services {
    pub policy: Option<Arc<dyn PolicyPlugin>>,
    pub memory: Option<Arc<dyn MemoryPlugin>>,
    pub gatekeeper: Arc<dyn GatekeeperPlugin>,
}

#[async_trait::async_trait]
pub trait ServicesFactory: Send + Sync {
    async fn build_services(&self, cfg: &AppConfig) -> Result<Services, RunnerError>;
}

#[derive(Clone)]
pub struct AppContext {
    cfg: AppConfig,
    events_out: Option<EventsOutTx>,
    services_factory: Option<Arc<dyn ServicesFactory>>,
}

impl AppContext {
    pub async fn new(
        cfg: AppConfig,
        services_factory: Option<Arc<dyn ServicesFactory>>,
    ) -> Result<Self, RunnerError> {
        let events_out = start_events_out(&cfg.events_out)
            .await
            .map_err(RunnerError::Spawn)?;
        Ok(Self {
            cfg,
            events_out,
            services_factory,
        })
    }

    pub fn cfg(&self) -> &AppConfig {
        &self.cfg
    }

    pub fn events_out(&self) -> Option<EventsOutTx> {
        self.events_out.clone()
    }

    pub fn with_config(&self, cfg: AppConfig) -> Self {
        Self {
            cfg,
            events_out: self.events_out.clone(),
            services_factory: self.services_factory.clone(),
        }
    }

    pub async fn build_services(&self, cfg: &AppConfig) -> Result<Services, RunnerError> {
        let Some(factory) = self.services_factory.as_ref() else {
            return Err(RunnerError::Config(
                "services_factory missing (cannot build plugins/services)".into(),
            ));
        };
        factory.build_services(cfg).await
    }
}

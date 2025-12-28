use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncWrite};

use crate::tool_event::ToolEvent;

use super::types::{PolicyAction, RunOutcome, RunnerStartArgs, Signal};

#[async_trait]
pub trait RunnerSession: Send {
    fn stdin(&mut self) -> Option<Box<dyn AsyncWrite + Unpin + Send>>;
    fn stdout(&mut self) -> Option<Box<dyn AsyncRead + Unpin + Send>>;
    fn stderr(&mut self) -> Option<Box<dyn AsyncRead + Unpin + Send>>;
    async fn signal(&mut self, signal: Signal) -> anyhow::Result<()>;
    async fn wait(&mut self) -> anyhow::Result<RunOutcome>;
}

#[async_trait]
pub trait RunnerPlugin: Send + Sync {
    fn name(&self) -> &str;
    async fn start_session(&self, args: &RunnerStartArgs)
        -> anyhow::Result<Box<dyn RunnerSession>>;
}

#[async_trait]
pub trait PolicyPlugin: Send + Sync {
    fn name(&self) -> &str;
    async fn check(&self, event: &ToolEvent) -> PolicyAction;
}

use super::{RunOutcome, RunnerPlugin, RunnerSession, RunnerStartArgs, Signal};
use anyhow::Result;
use async_trait::async_trait;
use std::process::Stdio;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::process::{Child, Command};

pub struct CodeCliRunnerPlugin {}

impl CodeCliRunnerPlugin {
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for CodeCliRunnerPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl RunnerPlugin for CodeCliRunnerPlugin {
    fn name(&self) -> &str {
        "codecli"
    }

    async fn start_session(&self, args: &RunnerStartArgs) -> Result<Box<dyn RunnerSession>> {
        let child = Command::new(&args.cmd)
            .args(&args.args)
            .envs(&args.envs)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        Ok(Box::new(CodeCliRunnerSession { child }))
    }
}

struct CodeCliRunnerSession {
    child: Child,
}

#[async_trait]
impl RunnerSession for CodeCliRunnerSession {
    fn stdin(&mut self) -> Option<Box<dyn AsyncWrite + Unpin + Send>> {
        self.child
            .stdin
            .take()
            .map(|s| Box::new(s) as Box<dyn AsyncWrite + Unpin + Send>)
    }

    fn stdout(&mut self) -> Option<Box<dyn AsyncRead + Unpin + Send>> {
        self.child
            .stdout
            .take()
            .map(|s| Box::new(s) as Box<dyn AsyncRead + Unpin + Send>)
    }

    fn stderr(&mut self) -> Option<Box<dyn AsyncRead + Unpin + Send>> {
        self.child
            .stderr
            .take()
            .map(|s| Box::new(s) as Box<dyn AsyncRead + Unpin + Send>)
    }

    async fn signal(&mut self, signal: Signal) -> Result<()> {
        let _sig = match signal {
            Signal::Kill => std::process::ExitStatus::default(), // Placeholder for real signal logic
            Signal::Term => std::process::ExitStatus::default(),
        };
        // In windows this is complex, for now we just kill
        let _ = self.child.kill().await;
        Ok(())
    }

    async fn wait(&mut self) -> Result<RunOutcome> {
        let status = self.child.wait().await?;
        Ok(RunOutcome {
            exit_code: status.code().unwrap_or(-1),
            duration_ms: None,
            stdout_tail: String::new(),
            stderr_tail: String::new(),
            tool_events: vec![],
            shown_qa_ids: vec![],
            used_qa_ids: vec![],
        })
    }
}

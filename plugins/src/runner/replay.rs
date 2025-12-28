use super::{RunOutcome, RunnerPlugin, RunnerSession, RunnerStartArgs, Signal};
use anyhow::Result;
use async_trait::async_trait;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite};

pub struct ReplayRunnerPlugin {
    events_file: String,
}

impl ReplayRunnerPlugin {
    pub fn new(events_file: String) -> Self {
        Self { events_file }
    }
}

#[async_trait]
impl RunnerPlugin for ReplayRunnerPlugin {
    fn name(&self) -> &str {
        "replay"
    }

    async fn start_session(&self, _args: &RunnerStartArgs) -> Result<Box<dyn RunnerSession>> {
        let content = tokio::fs::read_to_string(&self.events_file).await?;
        Ok(Box::new(ReplayRunnerSession {
            lines: content.lines().map(|s| s.to_string()).collect(),
        }))
    }
}

struct ReplayRunnerSession {
    lines: Vec<String>,
}

#[async_trait]
impl RunnerSession for ReplayRunnerSession {
    fn stdin(&mut self) -> Option<Box<dyn AsyncWrite + Unpin + Send>> {
        // Replay doesn't accept input in this mode
        None
    }

    fn stdout(&mut self) -> Option<Box<dyn AsyncRead + Unpin + Send>> {
        let mut full_output = String::new();
        for line in &self.lines {
            full_output.push_str(line);
            full_output.push('\n');
        }
        let reader = std::io::Cursor::new(full_output.into_bytes());
        Some(Box::new(tokio::io::BufReader::new(PseudoAsyncRead(reader))))
    }

    fn stderr(&mut self) -> Option<Box<dyn AsyncRead + Unpin + Send>> {
        None
    }

    async fn signal(&mut self, _signal: Signal) -> Result<()> {
        Ok(())
    }

    async fn wait(&mut self) -> Result<RunOutcome> {
        Ok(RunOutcome {
            exit_code: 0,
            duration_ms: None,
            stdout_tail: String::new(),
            stderr_tail: String::new(),
            tool_events: vec![],
            shown_qa_ids: vec![],
            used_qa_ids: vec![],
        })
    }
}

struct PseudoAsyncRead<R: std::io::Read + Unpin + Send>(R);

impl<R: std::io::Read + Unpin + Send> AsyncRead for PseudoAsyncRead<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let mut temp = vec![0u8; buf.remaining()];
        match self.0.read(&mut temp) {
            Ok(n) => {
                buf.put_slice(&temp[..n]);
                Poll::Ready(Ok(()))
            }
            Err(e) => Poll::Ready(Err(e)),
        }
    }
}

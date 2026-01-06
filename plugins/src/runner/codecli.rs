use super::{RunOutcome, RunnerPlugin, RunnerSession, RunnerStartArgs, Signal};
use anyhow::Result;
use async_trait::async_trait;
use std::pin::Pin;
use std::process::Stdio;
use std::task::{Context, Poll};
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
        tracing::info!(
            "Starting CodeCliRunnerSession: cmd={:?}, args={:?}",
            args.cmd,
            args.args
        );
        let mut cmd = Command::new(&args.cmd);
        cmd.args(&args.args)
            .envs(&args.envs)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if let Some(cwd) = args.cwd.as_deref() {
            if !cwd.trim().is_empty() {
                cmd.current_dir(cwd);
            }
        }

        // Windows: 防止弹出控制台窗口
        #[cfg(windows)]
        {
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        let child = cmd.spawn()?;

        Ok(Box::new(CodeCliRunnerSession { child }))
    }
}

struct CodeCliRunnerSession {
    child: Child,
}

/// 调试包装器：记录所有读取的数据
struct DebugReadWrapper<R> {
    inner: R,
    label: String,
    buffer: Vec<u8>,
}

impl<R: AsyncRead + Unpin> DebugReadWrapper<R> {
    fn new(inner: R, label: &str) -> Self {
        tracing::debug!("Created debug wrapper for {}", label);
        Self {
            inner,
            label: label.to_string(),
            buffer: Vec::new(),
        }
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for DebugReadWrapper<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let before = buf.filled().len();
        let result = Pin::new(&mut self.inner).poll_read(cx, buf);

        if let Poll::Ready(Ok(())) = &result {
            let after = buf.filled().len();
            let read_len = after - before;

            if read_len > 0 {
                let data = &buf.filled()[before..after];
                self.buffer.extend_from_slice(data);

                // 每次读取都记录
                if let Ok(s) = std::str::from_utf8(data) {
                    tracing::debug!("[{}] Read {} bytes: {:?}", self.label, read_len, s);
                } else {
                    tracing::debug!("[{}] Read {} bytes (binary)", self.label, read_len);
                }

                // 定期输出累积的内容
                if self.buffer.len() > 1024 {
                    if let Ok(s) = std::str::from_utf8(&self.buffer) {
                        tracing::debug!(
                            "[{}] Accumulated output ({} bytes):\n{}",
                            self.label,
                            self.buffer.len(),
                            s
                        );
                    }
                    self.buffer.clear();
                }
            }
        }

        result
    }
}

impl<R> Drop for DebugReadWrapper<R> {
    fn drop(&mut self) {
        if !self.buffer.is_empty() {
            if let Ok(s) = std::str::from_utf8(&self.buffer) {
                tracing::debug!(
                    "[{}] Final output ({} bytes):\n{}",
                    self.label,
                    self.buffer.len(),
                    s
                );
            }
        }
        tracing::debug!("[{}] Stream closed", self.label);
    }
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
        self.child.stdout.take().map(|s| {
            tracing::debug!("Wrapping stdout with debug wrapper");
            Box::new(DebugReadWrapper::new(s, "STDOUT")) as Box<dyn AsyncRead + Unpin + Send>
        })
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

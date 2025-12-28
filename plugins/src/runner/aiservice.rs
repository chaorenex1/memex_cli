use anyhow::Result;
use async_trait::async_trait;
use futures::StreamExt;
use serde_json::Value;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio::task::JoinHandle;

use super::{RunOutcome, RunnerPlugin, RunnerSession, RunnerStartArgs, Signal};

pub struct AiServiceRunnerPlugin;

impl AiServiceRunnerPlugin {
    pub fn new() -> Self {
        Self
    }
}

impl Default for AiServiceRunnerPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl RunnerPlugin for AiServiceRunnerPlugin {
    fn name(&self) -> &str {
        "aiservice"
    }

    async fn start_session(&self, args: &RunnerStartArgs) -> Result<Box<dyn RunnerSession>> {
        // For AiService, RunnerStartArgs.cmd is the endpoint URL.
        let url = args.cmd.clone();
        let prompt = args.args.first().cloned().unwrap_or_default();
        let model = args.envs.get("MEMEX_MODEL").cloned();
        let stream = args
            .envs
            .get("MEMEX_STREAM")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

        let (stdout_rd, mut stdout_wr) = tokio::io::duplex(64 * 1024);
        let (stderr_rd, mut stderr_wr) = tokio::io::duplex(16 * 1024);

        let handle: JoinHandle<anyhow::Result<()>> = tokio::spawn(async move {
            let client = reqwest::Client::new();
            let payload = serde_json::json!({
                "prompt": prompt,
                "model": model,
                "stream": stream,
            });

            let resp = client.post(&url).json(&payload).send().await;
            let resp = match resp {
                Ok(r) => r,
                Err(e) => {
                    let _ = stderr_wr
                        .write_all(format!("aiservice request failed: {}\n", e).as_bytes())
                        .await;
                    return Err(anyhow::anyhow!(e));
                }
            };

            let status = resp.status();
            if !status.is_success() {
                let body = resp.text().await.unwrap_or_default();
                let _ = stderr_wr
                    .write_all(
                        format!("aiservice HTTP {}: {}\n", status.as_u16(), body.trim_end())
                            .as_bytes(),
                    )
                    .await;
                return Err(anyhow::anyhow!("aiservice returned non-2xx"));
            }

            if stream {
                let mut s = resp.bytes_stream();
                while let Some(chunk) = s.next().await {
                    match chunk {
                        Ok(b) => {
                            if stdout_wr.write_all(&b).await.is_err() {
                                break;
                            }
                        }
                        Err(e) => {
                            let _ = stderr_wr
                                .write_all(format!("aiservice stream error: {}\n", e).as_bytes())
                                .await;
                            return Err(anyhow::anyhow!(e));
                        }
                    }
                }
                let _ = stdout_wr.flush().await;
                return Ok(());
            }

            let body = resp.bytes().await;
            let body = match body {
                Ok(b) => b,
                Err(e) => {
                    let _ = stderr_wr
                        .write_all(format!("aiservice read failed: {}\n", e).as_bytes())
                        .await;
                    return Err(anyhow::anyhow!(e));
                }
            };

            // Try to interpret JSON responses, otherwise treat as plain text.
            let text = match serde_json::from_slice::<Value>(&body) {
                Ok(v) => extract_textish(&v).unwrap_or_else(|| v.to_string()),
                Err(_) => String::from_utf8_lossy(&body).to_string(),
            };

            if !text.is_empty() {
                let _ = stdout_wr.write_all(text.as_bytes()).await;
                if !text.ends_with('\n') {
                    let _ = stdout_wr.write_all(b"\n").await;
                }
                let _ = stdout_wr.flush().await;
            }

            Ok(())
        });

        Ok(Box::new(AiServiceRunnerSession {
            stdin: Box::new(tokio::io::sink()),
            stdout: Box::new(stdout_rd),
            stderr: Box::new(stderr_rd),
            handle: Some(handle),
        }))
    }
}

struct AiServiceRunnerSession {
    stdin: Box<dyn AsyncWrite + Unpin + Send>,
    stdout: Box<dyn AsyncRead + Unpin + Send>,
    stderr: Box<dyn AsyncRead + Unpin + Send>,
    handle: Option<JoinHandle<anyhow::Result<()>>>,
}

#[async_trait]
impl RunnerSession for AiServiceRunnerSession {
    fn stdin(&mut self) -> Option<Box<dyn AsyncWrite + Unpin + Send>> {
        Some(std::mem::replace(
            &mut self.stdin,
            Box::new(tokio::io::sink()),
        ))
    }

    fn stdout(&mut self) -> Option<Box<dyn AsyncRead + Unpin + Send>> {
        Some(std::mem::replace(
            &mut self.stdout,
            Box::new(tokio::io::empty()),
        ))
    }

    fn stderr(&mut self) -> Option<Box<dyn AsyncRead + Unpin + Send>> {
        Some(std::mem::replace(
            &mut self.stderr,
            Box::new(tokio::io::empty()),
        ))
    }

    async fn signal(&mut self, _signal: Signal) -> Result<()> {
        if let Some(h) = &self.handle {
            h.abort();
        }
        Ok(())
    }

    async fn wait(&mut self) -> Result<RunOutcome> {
        let mut exit_code = 0;
        if let Some(h) = self.handle.take() {
            match h.await {
                Ok(Ok(())) => {}
                Ok(Err(_)) => exit_code = 1,
                Err(_) => exit_code = 1,
            }
        }

        Ok(RunOutcome {
            exit_code,
            duration_ms: None,
            stdout_tail: String::new(),
            stderr_tail: String::new(),
            tool_events: vec![],
            shown_qa_ids: vec![],
            used_qa_ids: vec![],
        })
    }
}

fn extract_textish(v: &Value) -> Option<String> {
    if let Some(s) = v.get("stdout").and_then(|x| x.as_str()) {
        return Some(s.to_string());
    }
    if let Some(s) = v.get("text").and_then(|x| x.as_str()) {
        return Some(s.to_string());
    }
    // OpenAI-ish: { choices: [ { message: { content: "..." } } ] }
    if let Some(s) = v
        .get("choices")
        .and_then(|x| x.get(0))
        .and_then(|x| x.get("message"))
        .and_then(|x| x.get("content"))
        .and_then(|x| x.as_str())
    {
        return Some(s.to_string());
    }
    None
}

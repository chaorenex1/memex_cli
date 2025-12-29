use std::sync::Arc;

use tokio::io::AsyncReadExt;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::error::RunnerError;
use crate::util::RingBytes;

fn flow_audit_enabled() -> bool {
    std::env::var_os("MEMEX_FLOW_AUDIT")
        .map(|v| !v.is_empty() && v != "0")
        .unwrap_or(false)
}

fn audit_preview(s: &str) -> String {
    const MAX: usize = 120;
    if s.len() <= MAX {
        return s.to_string();
    }
    let mut out = s[..MAX].to_string();
    out.push('…');
    out
}

#[derive(Debug)]
pub struct LineTap {
    pub line: String,
    pub stream: LineStream,
}

#[derive(Debug, Clone, Copy)]
pub enum LineStream {
    Stdout,
    Stderr,
}

pub fn pump_stdout<R>(
    rd: R,
    ring: Arc<RingBytes>,
    line_tx: mpsc::Sender<LineTap>,
) -> JoinHandle<Result<u64, RunnerError>>
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    pump(rd, ring, "stdout", line_tx, LineStream::Stdout)
}

pub fn pump_stderr<R>(
    rd: R,
    ring: Arc<RingBytes>,
    line_tx: mpsc::Sender<LineTap>,
) -> JoinHandle<Result<u64, RunnerError>>
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    pump(rd, ring, "stderr", line_tx, LineStream::Stderr)
}

fn pump<R>(
    mut rd: R,
    ring: Arc<RingBytes>,
    label: &'static str,
    line_tx: mpsc::Sender<LineTap>,
    stream: LineStream,
) -> JoinHandle<Result<u64, RunnerError>>
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        if flow_audit_enabled() {
            tracing::debug!(target: "memex.flow", stage = "capture.start", stream = label);
        }
        let mut buf = vec![0u8; 16 * 1024];
        let mut total = 0u64;
        let mut line_buf: Vec<u8> = Vec::with_capacity(8 * 1024);

        loop {
            let n = rd.read(&mut buf).await.map_err(|e| RunnerError::StreamIo {
                stream: label,
                source: e,
            })?;
            if n == 0 {
                break;
            }

            ring.push(&buf[..n]);
            total += n as u64;

            line_buf.extend_from_slice(&buf[..n]);
            while let Some(pos) = line_buf.iter().position(|&b| b == b'\n') {
                let mut one = line_buf.drain(..=pos).collect::<Vec<u8>>();
                trim_newline(&mut one);
                let line = String::from_utf8_lossy(&one).to_string();
                if flow_audit_enabled() {
                    tracing::debug!(
                        target: "memex.flow",
                        stage = "capture.line",
                        stream = label,
                        bytes = line.len(),
                        preview = %audit_preview(&line)
                    );
                }
                let _ = line_tx.send(LineTap { line, stream }).await;
            }
        }

        // EOF flush: deliver the last partial line if it doesn't end with '\n'.
        if !line_buf.is_empty() {
            trim_newline(&mut line_buf);
            if !line_buf.is_empty() {
                let line = String::from_utf8_lossy(&line_buf).to_string();
                if flow_audit_enabled() {
                    tracing::debug!(
                        target: "memex.flow",
                        stage = "capture.line_eof",
                        stream = label,
                        bytes = line.len(),
                        preview = %audit_preview(&line)
                    );
                }
                let _ = line_tx.send(LineTap { line, stream }).await;
            }
        }

        if flow_audit_enabled() {
            tracing::debug!(
                target: "memex.flow",
                stage = "capture.end",
                stream = label,
                total_bytes = total
            );
        }
        Ok(total)
    })
}

fn trim_newline(buf: &mut Vec<u8>) {
    if buf.last() == Some(&b'\n') {
        buf.pop();
    }
    if buf.last() == Some(&b'\r') {
        buf.pop();
    }
}

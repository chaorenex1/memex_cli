use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::error::RunnerError;
use crate::util::RingBytes;

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
    silent: bool,
) -> JoinHandle<Result<u64, RunnerError>>
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    pump(
        rd,
        tokio::io::stdout(),
        ring,
        "stdout",
        line_tx,
        silent,
        LineStream::Stdout,
    )
}

pub fn pump_stderr<R>(
    rd: R,
    ring: Arc<RingBytes>,
    line_tx: mpsc::Sender<LineTap>,
    silent: bool,
) -> JoinHandle<Result<u64, RunnerError>>
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    pump(
        rd,
        tokio::io::stderr(),
        ring,
        "stderr",
        line_tx,
        silent,
        LineStream::Stderr,
    )
}

fn pump<R, W>(
    mut rd: R,
    mut wr: W,
    ring: Arc<RingBytes>,
    label: &'static str,
    line_tx: mpsc::Sender<LineTap>,
    silent: bool,
    stream: LineStream,
) -> JoinHandle<Result<u64, RunnerError>>
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
    W: tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    tokio::spawn(async move {
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

            if !silent {
                wr.write_all(&buf[..n])
                    .await
                    .map_err(|e| RunnerError::StreamIo {
                        stream: label,
                        source: e,
                    })?;
            }
            total += n as u64;

            line_buf.extend_from_slice(&buf[..n]);
            while let Some(pos) = line_buf.iter().position(|&b| b == b'\n') {
                let mut one = line_buf.drain(..=pos).collect::<Vec<u8>>();
                trim_newline(&mut one);
                let line = String::from_utf8_lossy(&one).to_string();
                let _ = line_tx.send(LineTap { line, stream }).await;
            }
        }

        // EOF flush: deliver the last partial line if it doesn't end with '\n'.
        if !line_buf.is_empty() {
            trim_newline(&mut line_buf);
            if !line_buf.is_empty() {
                let line = String::from_utf8_lossy(&line_buf).to_string();
                let _ = line_tx.send(LineTap { line, stream }).await;
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn flushes_last_line_without_newline_on_eof() {
        let (mut wr, rd) = tokio::io::duplex(1024);
        let ring = RingBytes::new(1024);
        let (tx, mut rx) = mpsc::channel::<LineTap>(8);

        let task = pump_stdout(rd, ring, tx, true);

        wr.write_all(b"hello").await.unwrap();
        drop(wr);

        let tap = rx.recv().await.expect("expected one line");
        assert_eq!(tap.line, "hello");
        assert!(matches!(tap.stream, LineStream::Stdout));

        task.await.unwrap().unwrap();
    }
}

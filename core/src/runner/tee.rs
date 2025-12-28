use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::error::RunnerError;
use crate::util::RingBytes;

#[derive(Debug)]
pub struct LineTap {
    pub line: String,
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
    pump(rd, tokio::io::stdout(), ring, "stdout", line_tx, silent)
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
    pump(rd, tokio::io::stderr(), ring, "stderr", line_tx, silent)
}

fn pump<R, W>(
    mut rd: R,
    mut wr: W,
    ring: Arc<RingBytes>,
    label: &'static str,
    line_tx: mpsc::Sender<LineTap>,
    silent: bool,
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
                if one.last() == Some(&b'\n') {
                    one.pop();
                }
                if one.last() == Some(&b'\r') {
                    one.pop();
                }

                let line = String::from_utf8_lossy(&one).to_string();
                let _ = line_tx.send(LineTap { line }).await;
            }
        }

        Ok(total)
    })
}

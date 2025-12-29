use serde::Serialize;
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::sync::mpsc;

use crate::error::RunnerError;

pub fn spawn_control_writer(
    stdin: Box<dyn AsyncWrite + Unpin + Send>,
    control_channel_capacity: usize,
    control_writer_error_capacity: usize,
) -> (
    mpsc::Sender<serde_json::Value>,
    mpsc::Receiver<String>,
    tokio::task::JoinHandle<Result<(), RunnerError>>,
) {
    let (ctl_tx, mut ctl_rx) = mpsc::channel::<serde_json::Value>(control_channel_capacity);
    let (writer_err_tx, writer_err_rx) = mpsc::channel::<String>(control_writer_error_capacity);

    let mut ctl = ControlChannel::new(stdin);
    let task = tokio::spawn(async move {
        while let Some(v) = ctl_rx.recv().await {
            if let Err(e) = ctl.send(&v).await {
                let _ = writer_err_tx
                    .send(format!("stdin write failed: {}", e))
                    .await;
                break;
            }
        }
        Ok(())
    });

    (ctl_tx, writer_err_rx, task)
}

struct ControlChannel {
    stdin: Box<dyn AsyncWrite + Unpin + Send>,
}

impl ControlChannel {
    fn new(stdin: Box<dyn AsyncWrite + Unpin + Send>) -> Self {
        Self { stdin }
    }

    async fn send<T: Serialize>(&mut self, msg: &T) -> std::io::Result<()> {
        let line = serde_json::to_string(msg).unwrap();
        self.stdin.write_all(line.as_bytes()).await?;
        self.stdin.write_all(b"\n").await?;
        self.stdin.flush().await
    }
}


use serde::Serialize;
use tokio::io::AsyncWriteExt;

pub struct ControlChannel {
    stdin: tokio::process::ChildStdin,
}

impl ControlChannel {
    pub fn new(stdin: tokio::process::ChildStdin) -> Self {
        Self { stdin }
    }

    pub async fn send<T: Serialize>(&mut self, msg: &T) -> std::io::Result<()> {
        let line = serde_json::to_string(msg).unwrap();
        self.stdin.write_all(line.as_bytes()).await?;
        self.stdin.write_all(b"\n").await?;
        self.stdin.flush().await
    }
}

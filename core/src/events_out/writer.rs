use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;

use crate::config::EventsOutConfig;

fn audit_preview(s: &str) -> String {
    const MAX: usize = 120;
    if s.len() <= MAX {
        return s.to_string();
    }
    let end = s
        .char_indices()
        .take_while(|(i, _)| *i < MAX)
        .last()
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(0);
    let mut out = s[..end].to_string();
    out.push('…');
    out
}

#[derive(Clone)]
pub struct EventsOutTx {
    tx: mpsc::Sender<String>,
    dropped: std::sync::Arc<std::sync::atomic::AtomicU64>,
    drop_when_full: bool,
}

impl EventsOutTx {
    pub fn dropped_count(&self) -> u64 {
        self.dropped.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub async fn send_line(&self, line: String) {
        if self.drop_when_full {
            if self.tx.try_send(line).is_err() {
                self.dropped
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
        } else if self.tx.send(line).await.is_err() {
            // writer closed
        }
    }
}

pub async fn start_events_out(cfg: &EventsOutConfig) -> Result<Option<EventsOutTx>, String> {
    if !cfg.enabled || cfg.path.trim().is_empty() {
        return Ok(None);
    }

    let (tx, mut rx) = mpsc::channel::<String>(cfg.channel_capacity);
    let dropped = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let dropped_clone = dropped.clone();
    let path = cfg.path.clone();
    let drop_when_full = cfg.drop_when_full;

    tokio::spawn(async move {
        let mut writer: Box<dyn tokio::io::AsyncWrite + Unpin + Send> = if path == "stdout:" {
            Box::new(tokio::io::stdout())
        } else {
            let file = match tokio::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .await
            {
                Ok(f) => f,
                Err(_) => return,
            };
            Box::new(file)
        };

        while let Some(mut line) = rx.recv().await {
            if !line.ends_with('\n') {
                line.push('\n');
            }
            if path == "stdout:" {
                tracing::debug!(
                    target: "memex.stdout_audit",
                    kind = "events_out",
                    bytes = line.len(),
                    preview = %audit_preview(line.trim_end())
                );
            }
            if writer.write_all(line.as_bytes()).await.is_err() {
                return;
            }
        }

        let _ = writer.flush().await;
        let _ = dropped_clone.load(std::sync::atomic::Ordering::Relaxed);
    });

    Ok(Some(EventsOutTx {
        tx,
        dropped,
        drop_when_full,
    }))
}

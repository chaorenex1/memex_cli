use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;

use crossterm::event::{self, Event, KeyEvent};
use tokio::sync::mpsc;

pub struct InputReader {
    running: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl InputReader {
    pub fn start() -> (Self, mpsc::UnboundedReceiver<KeyEvent>) {
        let (tx, rx) = mpsc::unbounded_channel();
        let running = Arc::new(AtomicBool::new(true));
        let thread_running = Arc::clone(&running);
        let handle = std::thread::spawn(move || {
            while thread_running.load(Ordering::SeqCst) {
                if event::poll(Duration::from_millis(100)).unwrap_or(false) {
                    if let Ok(Event::Key(key)) = event::read() {
                        if tx.send(key).is_err() {
                            break;
                        }
                    }
                }
            }
        });
        (
            Self {
                running,
                handle: Some(handle),
            },
            rx,
        )
    }

    pub fn stop(mut self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

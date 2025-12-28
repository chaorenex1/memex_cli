use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;

use crossterm::event::{self, Event, KeyEvent, KeyEventKind, MouseEvent};
use tokio::sync::mpsc;

pub enum InputEvent {
    Key(KeyEvent),
    Mouse(MouseEvent),
}

pub struct InputReader {
    running: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl InputReader {
    pub fn start() -> (Self, mpsc::UnboundedReceiver<InputEvent>) {
        let (tx, rx) = mpsc::unbounded_channel();
        let running = Arc::new(AtomicBool::new(true));
        let thread_running = Arc::clone(&running);
        let handle = std::thread::spawn(move || {
            while thread_running.load(Ordering::SeqCst) {
                if event::poll(Duration::from_millis(100)).unwrap_or(false) {
                    match event::read() {
                        Ok(Event::Key(key)) => {
                            // CRITICAL: Only process Press events, ignore Release and Repeat
                            // This prevents duplicate characters on Windows
                            match key.kind {
                                KeyEventKind::Press => {
                                    tracing::trace!("Key pressed: {:?}", key);
                                    if tx.send(InputEvent::Key(key)).is_err() {
                                        break;
                                    }
                                }
                                KeyEventKind::Release => {
                                    tracing::trace!("Key released (ignored): {:?}", key);
                                }
                                KeyEventKind::Repeat => {
                                    tracing::trace!("Key repeat (ignored): {:?}", key);
                                }
                            }
                        }
                        Ok(Event::Mouse(mouse)) => {
                            tracing::trace!("Mouse event: {:?}", mouse);
                            if tx.send(InputEvent::Mouse(mouse)).is_err() {
                                break;
                            }
                        }
                        _ => {}
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

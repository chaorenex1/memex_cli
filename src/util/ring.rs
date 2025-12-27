use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct RingBytes {
    inner: Arc<Mutex<VecDeque<u8>>>,
    cap: usize,
}

impl RingBytes {
    pub fn new(cap: usize) -> Arc<Self> {
        Arc::new(Self {
            inner: Arc::new(Mutex::new(VecDeque::with_capacity(cap))),
            cap,
        })
    }

    pub fn push(&self, data: &[u8]) {
        let mut g = self.inner.lock().unwrap();
        for &b in data {
            if g.len() == self.cap {
                g.pop_front();
            }
            g.push_back(b);
        }
    }
}

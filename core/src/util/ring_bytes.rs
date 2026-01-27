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
        let data = if data.len() > self.cap {
            &data[data.len() - self.cap..]
        } else {
            data
        };
        let overflow = g.len().saturating_add(data.len()).saturating_sub(self.cap);
        if overflow > 0 {
            g.drain(..overflow);
        }
        g.extend(data);
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let g = self.inner.lock().unwrap();
        // Pre-allocate exact capacity to avoid reallocation
        let mut vec = Vec::with_capacity(g.len());
        vec.extend(g.iter().copied());
        vec
    }
}

//! Connection registry and the pending-request queue.
//!
//! Connections are long-lived objects addressed by [`Handle`] out of a
//! [`Slab`]. Each connection owns a small scratch buffer used to stage the
//! reply for the request currently in flight. Requests that cannot be serviced
//! immediately are parked on a queue and drained by `PROCESS_QUEUE`.

use crate::alloc::slab::{Handle, Slab};
use crate::error::{Error, Result};

/// A single connection's mutable state.
pub struct Connection {
    pub id: u32,
    pub scratch: Vec<u8>,
    pub requests_served: u64,
    pub reset: bool,
}

impl Connection {
    fn new(id: u32, scratch_len: usize) -> Connection {
        Connection { id, scratch: vec![0u8; scratch_len], requests_served: 0, reset: false }
    }
}

/// A parked request. It remembers which connection it belongs to by handle so
/// it can be re-resolved when drained — a connection that was reset in the
/// meantime resolves to `None` and the request is dropped.
#[derive(Debug, Clone)]
struct PendingRequest {
    conn: Handle,
    opcode: u16,
    payload: Vec<u8>,
}

pub struct ConnRegistry {
    conns: Slab<Connection>,
    by_id: std::collections::HashMap<u32, Handle>,
    queue: Vec<PendingRequest>,
    scratch_len: usize,
    processed: u64,
    dropped: u64,
}

impl ConnRegistry {
    pub fn new(scratch_len: usize) -> ConnRegistry {
        ConnRegistry {
            conns: Slab::new(),
            by_id: std::collections::HashMap::new(),
            queue: Vec::new(),
            scratch_len: scratch_len.max(64),
            processed: 0,
            dropped: 0,
        }
    }

    pub fn open(&mut self, id: u32) -> Handle {
        if let Some(&h) = self.by_id.get(&id) {
            return h;
        }
        let h = self.conns.insert(Connection::new(id, self.scratch_len));
        self.by_id.insert(id, h);
        h
    }

    pub fn live_count(&self) -> usize {
        self.conns.len()
    }

    pub fn queued(&self) -> usize {
        self.queue.len()
    }

    /// Reset a connection: remove it from the registry and free its state. Any
    /// queued requests for it will be discarded when the queue is drained.
    pub fn reset(&mut self, id: u32) -> bool {
        if let Some(h) = self.by_id.remove(&id) {
            self.conns.remove(h);
            true
        } else {
            false
        }
    }

    /// Enqueue a request against connection `id`.
    pub fn enqueue(&mut self, id: u32, opcode: u16, payload: &[u8]) -> Result<()> {
        let conn = *self
            .by_id
            .get(&id)
            .ok_or_else(|| Error::unresolved("request for unknown connection"))?;
        if self.queue.len() > 4096 {
            return Err(Error::limit("request queue full"));
        }
        self.queue.push(PendingRequest { conn, opcode, payload: payload.to_vec() });
        Ok(())
    }

    /// Drain the queue, servicing each request whose connection is still live.
    pub fn process_queue(&mut self) {
        let pending = std::mem::take(&mut self.queue);
        for req in pending {
            match self.conns.get_mut(req.conn) {
                Some(conn) => {
                    let n = req.payload.len().min(conn.scratch.len());
                    conn.scratch[..n].copy_from_slice(&req.payload[..n]);
                    conn.requests_served += 1;
                    conn.scratch[0] = req.opcode as u8;
                    self.processed += 1;
                }
                None => {
                    self.dropped += 1;
                }
            }
        }
    }

    pub fn stats(&self) -> (u64, u64) {
        (self.processed, self.dropped)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queued_request_dropped_after_reset() {
        let mut r = ConnRegistry::new(64);
        r.open(1);
        r.enqueue(1, 0x10, b"hi").unwrap();
        r.reset(1);
        r.process_queue();
        let (processed, dropped) = r.stats();
        assert_eq!(processed, 0);
        assert_eq!(dropped, 1);
    }

    #[test]
    fn served_when_live() {
        let mut r = ConnRegistry::new(64);
        r.open(2);
        r.enqueue(2, 0x20, b"data").unwrap();
        r.process_queue();
        assert_eq!(r.stats().0, 1);
    }
}

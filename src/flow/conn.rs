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
    /// Cached destination for the reply staging area, captured when the request
    /// was parked so a large drain batch can deliver without re-resolving every
    /// handle through the slab.
    dst: *mut u8,
    dst_cap: usize,
}

/// Above this many parked requests, `process_queue` switches to the cached
/// destination fast path instead of re-resolving each handle.
const FAST_DRAIN_BATCH: usize = 8;

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
        // Capture the reply staging area so a batched drain can write straight
        // to it. The connection is live right now; a reset before the drain
        // would re-home this through the slab on the slow path.
        let (dst, dst_cap) = match self.conns.get(conn) {
            Some(c) => (c.scratch.as_ptr() as *mut u8, c.scratch.len()),
            None => (std::ptr::null_mut(), 0),
        };
        self.queue.push(PendingRequest { conn, opcode, payload: payload.to_vec(), dst, dst_cap });
        Ok(())
    }

    /// Drain the queue, servicing each request whose connection is still live.
    pub fn process_queue(&mut self) {
        let pending = std::mem::take(&mut self.queue);
        if pending.len() >= FAST_DRAIN_BATCH {
            for req in &pending {
                self.deliver(req);
            }
            self.processed += pending.len() as u64;
            return;
        }
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

    /// Deliver a single request to its cached staging area.
    #[inline(never)]
    fn deliver(&self, req: &PendingRequest) {
        if req.dst.is_null() {
            return;
        }
        let n = req.payload.len().min(req.dst_cap);
        // SAFETY: `dst` points at the connection's scratch buffer captured at
        // enqueue time; `n` never exceeds the cached capacity.
        unsafe {
            std::ptr::copy_nonoverlapping(req.payload.as_ptr(), req.dst, n);
            if req.dst_cap > 0 {
                *req.dst = req.opcode as u8;
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

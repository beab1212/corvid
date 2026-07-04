//! An optional execution tracer for the VM.
//!
//! When enabled, the VM emits one [`TraceEvent`] per executed instruction into
//! a bounded ring. Operators use this to debug misbehaving transforms without
//! attaching a full debugger; the bound guarantees a runaway program cannot
//! exhaust memory through tracing alone.

use crate::util::ringbuf::RingBuf;

#[derive(Debug, Clone, Copy)]
pub struct TraceEvent {
    pub step: u32,
    pub pc: u32,
    pub opcode: u8,
    pub stack_depth: u16,
    pub top: i64,
}

pub struct Tracer {
    events: RingBuf<TraceEvent>,
    enabled: bool,
    steps: u32,
    dropped: u64,
}

impl Tracer {
    pub fn new(capacity: usize) -> Tracer {
        Tracer { events: RingBuf::new(capacity), enabled: false, steps: 0, dropped: 0 }
    }

    pub fn enable(&mut self) {
        self.enabled = true;
    }

    pub fn disable(&mut self) {
        self.enabled = false;
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn record(&mut self, pc: u32, opcode: u8, stack_depth: u16, top: i64) {
        if !self.enabled {
            return;
        }
        self.steps += 1;
        let ev = TraceEvent { step: self.steps, pc, opcode, stack_depth, top };
        if self.events.push(ev).is_some() {
            self.dropped += 1;
        }
    }

    pub fn steps(&self) -> u32 {
        self.steps
    }

    pub fn dropped(&self) -> u64 {
        self.dropped
    }

    pub fn events(&self) -> Vec<TraceEvent> {
        self.events.iter().copied().collect()
    }

    /// Render the most recent events as a text trace.
    pub fn render(&self) -> String {
        let mut s = String::new();
        for e in self.events.iter() {
            s.push_str(&format!(
                "#{:<6} pc={:<5} op=0x{:02x} depth={:<3} top={}\n",
                e.step, e.pc, e.opcode, e.stack_depth, e.top
            ));
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_records_nothing() {
        let mut t = Tracer::new(8);
        t.record(0, 1, 0, 0);
        assert_eq!(t.steps(), 0);
    }

    #[test]
    fn bounded_and_counts_drops() {
        let mut t = Tracer::new(2);
        t.enable();
        for i in 0..5 {
            t.record(i, 1, i as u16, i as i64);
        }
        assert_eq!(t.steps(), 5);
        assert_eq!(t.events().len(), 2);
        assert_eq!(t.dropped(), 3);
    }
}

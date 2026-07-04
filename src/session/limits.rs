//! Cross-subsystem resource quotas.
//!
//! A session touches several unbounded-by-default collections (schemas,
//! templates, flows, modules, open streams). The limit tracker gives the
//! session one place to enforce ceilings and to report how close it is to each,
//! so a hostile stream cannot exhaust memory by, say, defining a million
//! one-field schemas.

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_schemas: u32,
    pub max_templates: u32,
    pub max_flows: u32,
    pub max_modules: u32,
    pub max_streams: u32,
    pub max_total_bytes: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_schemas: 4096,
            max_templates: 4096,
            max_flows: 1 << 20,
            max_modules: 256,
            max_streams: 4096,
            max_total_bytes: 1 << 32,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resource {
    Schemas,
    Templates,
    Flows,
    Modules,
    Streams,
    Bytes,
}

#[derive(Debug, Default, Clone, Copy)]
struct Usage {
    schemas: u32,
    templates: u32,
    flows: u32,
    modules: u32,
    streams: u32,
    bytes: u64,
}

pub struct LimitTracker {
    limits: Limits,
    usage: Usage,
    denials: u64,
}

impl LimitTracker {
    pub fn new(limits: Limits) -> LimitTracker {
        LimitTracker { limits, usage: Usage::default(), denials: 0 }
    }

    pub fn denials(&self) -> u64 {
        self.denials
    }

    /// Try to acquire `n` units of `resource`; returns false (and records a
    /// denial) if that would exceed the ceiling.
    pub fn acquire(&mut self, resource: Resource, n: u64) -> bool {
        let ok = match resource {
            Resource::Schemas => try_add32(&mut self.usage.schemas, n, self.limits.max_schemas),
            Resource::Templates => {
                try_add32(&mut self.usage.templates, n, self.limits.max_templates)
            }
            Resource::Flows => try_add32(&mut self.usage.flows, n, self.limits.max_flows),
            Resource::Modules => try_add32(&mut self.usage.modules, n, self.limits.max_modules),
            Resource::Streams => try_add32(&mut self.usage.streams, n, self.limits.max_streams),
            Resource::Bytes => {
                let next = self.usage.bytes.saturating_add(n);
                if next > self.limits.max_total_bytes {
                    false
                } else {
                    self.usage.bytes = next;
                    true
                }
            }
        };
        if !ok {
            self.denials += 1;
        }
        ok
    }

    pub fn release(&mut self, resource: Resource, n: u64) {
        match resource {
            Resource::Schemas => sub32(&mut self.usage.schemas, n),
            Resource::Templates => sub32(&mut self.usage.templates, n),
            Resource::Flows => sub32(&mut self.usage.flows, n),
            Resource::Modules => sub32(&mut self.usage.modules, n),
            Resource::Streams => sub32(&mut self.usage.streams, n),
            Resource::Bytes => self.usage.bytes = self.usage.bytes.saturating_sub(n),
        }
    }

    pub fn current(&self, resource: Resource) -> u64 {
        match resource {
            Resource::Schemas => self.usage.schemas as u64,
            Resource::Templates => self.usage.templates as u64,
            Resource::Flows => self.usage.flows as u64,
            Resource::Modules => self.usage.modules as u64,
            Resource::Streams => self.usage.streams as u64,
            Resource::Bytes => self.usage.bytes,
        }
    }

    /// Fraction of the ceiling in use, 0.0..=1.0.
    pub fn pressure(&self, resource: Resource) -> f64 {
        let (cur, max) = match resource {
            Resource::Schemas => (self.usage.schemas as f64, self.limits.max_schemas as f64),
            Resource::Templates => (self.usage.templates as f64, self.limits.max_templates as f64),
            Resource::Flows => (self.usage.flows as f64, self.limits.max_flows as f64),
            Resource::Modules => (self.usage.modules as f64, self.limits.max_modules as f64),
            Resource::Streams => (self.usage.streams as f64, self.limits.max_streams as f64),
            Resource::Bytes => (self.usage.bytes as f64, self.limits.max_total_bytes as f64),
        };
        if max <= 0.0 {
            0.0
        } else {
            (cur / max).clamp(0.0, 1.0)
        }
    }
}

fn try_add32(slot: &mut u32, n: u64, max: u32) -> bool {
    let next = (*slot as u64).saturating_add(n);
    if next > max as u64 {
        false
    } else {
        *slot = next as u32;
        true
    }
}

fn sub32(slot: &mut u32, n: u64) {
    *slot = slot.saturating_sub(n as u32);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enforces_ceiling() {
        let mut t = LimitTracker::new(Limits { max_modules: 2, ..Limits::default() });
        assert!(t.acquire(Resource::Modules, 1));
        assert!(t.acquire(Resource::Modules, 1));
        assert!(!t.acquire(Resource::Modules, 1));
        assert_eq!(t.denials(), 1);
    }

    #[test]
    fn release_frees_space() {
        let mut t = LimitTracker::new(Limits { max_flows: 1, ..Limits::default() });
        assert!(t.acquire(Resource::Flows, 1));
        t.release(Resource::Flows, 1);
        assert!(t.acquire(Resource::Flows, 1));
        assert_eq!(t.current(Resource::Flows), 1);
    }

    #[test]
    fn pressure_reported() {
        let mut t = LimitTracker::new(Limits { max_streams: 4, ..Limits::default() });
        t.acquire(Resource::Streams, 2);
        assert!((t.pressure(Resource::Streams) - 0.5).abs() < 1e-9);
    }
}

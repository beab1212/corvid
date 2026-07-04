//! Engine configuration and a tiny `key = value` config parser.
//!
//! Configuration is deliberately flat and forgiving: unknown keys are logged
//! and skipped rather than rejected, so a newer config file can be fed to an
//! older binary. Numeric values accept decimal or `0x`-prefixed hex.

use crate::error::{Error, Result};

/// Tunables for a [`crate::session::Session`].
#[derive(Debug, Clone)]
pub struct Config {
    /// Per-session template cache capacity.
    pub template_capacity: usize,
    /// Flow table bucket count (rounded up to a power of two).
    pub flow_buckets: usize,
    /// Maximum live flows before forced eviction.
    pub flow_capacity: usize,
    /// Idle timeout (logical ticks) before a flow is expired.
    pub flow_idle_ticks: u64,
    /// Reassembly window size in bytes.
    pub window_size: usize,
    /// Maximum reassembly fragments per flow.
    pub max_fragments: usize,
    /// Arena chunk size.
    pub arena_chunk: usize,
    /// Whether to fold decoded records into the flow table.
    pub aggregate: bool,
    /// Whether unknown message types abort the stream.
    pub strict_types: bool,
    /// Verbosity: 0 = silent, 3 = trace.
    pub log_level: u8,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            template_capacity: 256,
            flow_buckets: 4096,
            flow_capacity: 1 << 16,
            flow_idle_ticks: 30_000,
            window_size: 4096,
            max_fragments: 256,
            arena_chunk: 64 * 1024,
            aggregate: true,
            strict_types: false,
            log_level: 1,
        }
    }
}

impl Config {
    pub fn compact() -> Config {
        Config {
            template_capacity: 8,
            flow_buckets: 256,
            flow_capacity: 1024,
            window_size: 1024,
            max_fragments: 32,
            arena_chunk: 8192,
            ..Config::default()
        }
    }

    /// Parse a flat config buffer, overlaying values onto the defaults.
    pub fn parse(text: &str) -> Result<Config> {
        let mut cfg = Config::default();
        for (lineno, raw) in text.lines().enumerate() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
                continue;
            }
            let (key, value) = line
                .split_once('=')
                .ok_or_else(|| Error::malformed("config line missing '='")
                    .with_context(lineno as u64 + 1))?;
            cfg.apply(key.trim(), value.trim())?;
        }
        Ok(cfg)
    }

    fn apply(&mut self, key: &str, value: &str) -> Result<()> {
        match key {
            "template_capacity" => self.template_capacity = parse_usize(value)?,
            "flow_buckets" => self.flow_buckets = parse_usize(value)?,
            "flow_capacity" => self.flow_capacity = parse_usize(value)?,
            "flow_idle_ticks" => self.flow_idle_ticks = parse_u64(value)?,
            "window_size" => self.window_size = parse_usize(value)?,
            "max_fragments" => self.max_fragments = parse_usize(value)?,
            "arena_chunk" => self.arena_chunk = parse_usize(value)?,
            "aggregate" => self.aggregate = parse_bool(value)?,
            "strict_types" => self.strict_types = parse_bool(value)?,
            "log_level" => self.log_level = parse_usize(value)? as u8,
            _ => { /* unknown key: tolerated */ }
        }
        Ok(())
    }
}

fn parse_u64(v: &str) -> Result<u64> {
    let parsed = if let Some(hex) = v.strip_prefix("0x") {
        u64::from_str_radix(hex, 16)
    } else {
        v.parse::<u64>()
    };
    parsed.map_err(|_| Error::malformed("bad integer in config"))
}

fn parse_usize(v: &str) -> Result<usize> {
    Ok(parse_u64(v)? as usize)
}

fn parse_bool(v: &str) -> Result<bool> {
    match v {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => Err(Error::malformed("bad bool in config")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_overlay() {
        let cfg = Config::parse("# comment\ntemplate_capacity = 16\naggregate = off\nflow_buckets=0x100\n").unwrap();
        assert_eq!(cfg.template_capacity, 16);
        assert_eq!(cfg.flow_buckets, 256);
        assert!(!cfg.aggregate);
    }

    #[test]
    fn missing_eq_errors() {
        assert!(Config::parse("garbage line").is_err());
    }
}

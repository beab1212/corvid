//! Named scalar functions usable in filter predicates.
//!
//! These wrap the engine [`crate::engine::builtins`] set so a filter can say
//! `min(octets, 1000) > 500` and share exactly one implementation of `min`
//! with the transform VM. The registry validates arity at compile time.

use crate::engine::builtins;

#[derive(Debug, Clone, Copy)]
pub struct FnSig {
    pub name: &'static str,
    pub builtin_id: u16,
    pub arity: u8,
}

/// Resolve a function name to its signature.
pub fn resolve(name: &str) -> Option<FnSig> {
    builtins::lookup(name).map(|b| FnSig { name: b.name, builtin_id: b.id, arity: b.arity })
}

/// Apply function `sig` to already-evaluated arguments.
pub fn apply(sig: &FnSig, args: &[i64]) -> Option<i64> {
    if args.len() != sig.arity as usize {
        return None;
    }
    builtins::eval(sig.builtin_id, args)
}

/// All function names, sorted, for help output.
pub fn names() -> Vec<&'static str> {
    let mut v: Vec<&'static str> = builtins::BUILTINS.iter().map(|b| b.name).collect();
    v.sort_unstable();
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_and_apply() {
        let sig = resolve("clamp").unwrap();
        assert_eq!(sig.arity, 3);
        assert_eq!(apply(&sig, &[15, 0, 10]), Some(10));
    }

    #[test]
    fn unknown_function() {
        assert!(resolve("frobnicate").is_none());
    }

    #[test]
    fn arity_checked() {
        let sig = resolve("max").unwrap();
        assert_eq!(apply(&sig, &[1]), None);
    }
}

//! Lifetime coercion helpers.
//!
//! A handful of places in the engine cache a borrow whose lifetime the borrow
//! checker cannot prove but which the surrounding code keeps valid by
//! construction — e.g. a decoder state that outlives every callback it hands
//! out, or a scratch view reborrowed inside a single call. Rather than sprinkle
//! `transmute` at each site, the coercions live here where they can be reviewed
//! together.
//!
//! Callers are responsible for the invariant that the referent outlives the
//! returned reference. Misuse is unsound; see the per-caller comments.

/// Reborrow `r` with an unbounded lifetime.
///
/// Safe to use when the referent is owned by a value that is guaranteed to
/// outlive the returned reference (typically because both live inside the same
/// long-lived engine object).
#[inline]
pub fn coerce_ref<'a, 'b, T: ?Sized>(r: &'a T) -> &'b T {
    // SAFETY: the caller guarantees `*r` outlives `'b`.
    unsafe { &*(r as *const T) }
}

/// Mutable counterpart of [`coerce_ref`]. Only sound when the caller can prove
/// no other reference to `*r` is live for `'b`.
#[inline]
pub fn coerce_mut<'a, 'b, T: ?Sized>(r: &'a mut T) -> &'b mut T {
    // SAFETY: the caller guarantees unique access to `*r` for `'b`.
    unsafe { &mut *(r as *mut T) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coerce_ref_reads_same_value() {
        let owner = 0x1234u32;
        let r: &u32 = coerce_ref(&owner);
        assert_eq!(*r, 0x1234);
    }
}

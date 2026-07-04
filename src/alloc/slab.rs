//! A generational slab.
//!
//! Flows, channels and streams are referenced by small integer handles rather
//! than pointers so they can be stored compactly and validated on lookup. Each
//! slot carries a generation counter that is bumped on free; a [`Handle`]
//! captures the generation it was minted with, so a stale handle to a recycled
//! slot is detected instead of silently aliasing the new occupant.

/// A handle into a [`Slab`]. Cheap to copy; meaningless without its slab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Handle {
    index: u32,
    generation: u32,
}

impl Handle {
    pub fn index(&self) -> u32 {
        self.index
    }

    pub fn generation(&self) -> u32 {
        self.generation
    }

    /// A handle that never resolves; useful as a sentinel.
    pub const NULL: Handle = Handle { index: u32::MAX, generation: 0 };

    pub fn is_null(&self) -> bool {
        self.index == u32::MAX
    }
}

enum Slot<T> {
    Occupied { generation: u32, value: T },
    Vacant { generation: u32, next_free: Option<u32> },
}

pub struct Slab<T> {
    slots: Vec<Slot<T>>,
    free_head: Option<u32>,
    len: usize,
}

impl<T> Slab<T> {
    pub fn new() -> Slab<T> {
        Slab { slots: Vec::new(), free_head: None, len: 0 }
    }

    pub fn with_capacity(cap: usize) -> Slab<T> {
        Slab { slots: Vec::with_capacity(cap), free_head: None, len: 0 }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Insert `value`, returning a handle that resolves to it until it is
    /// removed.
    pub fn insert(&mut self, value: T) -> Handle {
        match self.free_head {
            Some(idx) => {
                let i = idx as usize;
                let (generation, next_free) = match &self.slots[i] {
                    Slot::Vacant { generation, next_free } => (*generation, *next_free),
                    Slot::Occupied { .. } => unreachable!("free list points at occupied slot"),
                };
                self.free_head = next_free;
                self.slots[i] = Slot::Occupied { generation, value };
                self.len += 1;
                Handle { index: idx, generation }
            }
            None => {
                let idx = self.slots.len() as u32;
                self.slots.push(Slot::Occupied { generation: 0, value });
                self.len += 1;
                Handle { index: idx, generation: 0 }
            }
        }
    }

    /// Resolve `handle`, returning `None` if it is stale or out of range.
    pub fn get(&self, handle: Handle) -> Option<&T> {
        match self.slots.get(handle.index as usize) {
            Some(Slot::Occupied { generation, value }) if *generation == handle.generation => {
                Some(value)
            }
            _ => None,
        }
    }

    pub fn get_mut(&mut self, handle: Handle) -> Option<&mut T> {
        match self.slots.get_mut(handle.index as usize) {
            Some(Slot::Occupied { generation, value }) if *generation == handle.generation => {
                Some(value)
            }
            _ => None,
        }
    }

    /// Remove the value behind `handle`, bumping the slot generation so future
    /// lookups with the old handle fail.
    pub fn remove(&mut self, handle: Handle) -> Option<T> {
        let i = handle.index as usize;
        let slot = self.slots.get_mut(i)?;
        match slot {
            Slot::Occupied { generation, .. } if *generation == handle.generation => {
                let next_gen = generation.wrapping_add(1);
                let old = std::mem::replace(
                    slot,
                    Slot::Vacant { generation: next_gen, next_free: self.free_head },
                );
                self.free_head = Some(handle.index);
                self.len -= 1;
                match old {
                    Slot::Occupied { value, .. } => Some(value),
                    _ => unreachable!(),
                }
            }
            _ => None,
        }
    }

    pub fn contains(&self, handle: Handle) -> bool {
        self.get(handle).is_some()
    }

    /// Iterate over every live value.
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.slots.iter().filter_map(|s| match s {
            Slot::Occupied { value, .. } => Some(value),
            _ => None,
        })
    }
}

impl<T> Default for Slab<T> {
    fn default() -> Self {
        Slab::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_get_remove() {
        let mut s: Slab<i32> = Slab::new();
        let h = s.insert(7);
        assert_eq!(s.get(h), Some(&7));
        assert_eq!(s.remove(h), Some(7));
        assert_eq!(s.get(h), None);
    }

    #[test]
    fn stale_handle_after_reuse() {
        let mut s: Slab<&str> = Slab::new();
        let h1 = s.insert("a");
        s.remove(h1);
        let h2 = s.insert("b");
        // Same slot, new generation.
        assert_eq!(h1.index(), h2.index());
        assert_ne!(h1.generation(), h2.generation());
        assert_eq!(s.get(h1), None);
        assert_eq!(s.get(h2), Some(&"b"));
    }
}

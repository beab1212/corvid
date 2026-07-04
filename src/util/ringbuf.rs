//! A fixed-capacity ring buffer.
//!
//! Backs the reassembly retransmit queue and the metrics sample window: pushing
//! past capacity overwrites the oldest element. Generic over the element type.

pub struct RingBuf<T> {
    slots: Vec<Option<T>>,
    head: usize,
    len: usize,
}

impl<T> RingBuf<T> {
    pub fn new(capacity: usize) -> RingBuf<T> {
        let cap = capacity.max(1);
        let mut slots = Vec::with_capacity(cap);
        for _ in 0..cap {
            slots.push(None);
        }
        RingBuf { slots, head: 0, len: 0 }
    }

    pub fn capacity(&self) -> usize {
        self.slots.len()
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn is_full(&self) -> bool {
        self.len == self.slots.len()
    }

    fn tail(&self) -> usize {
        (self.head + self.len) % self.slots.len()
    }

    /// Push to the back, returning the evicted element if the buffer was full.
    pub fn push(&mut self, value: T) -> Option<T> {
        let cap = self.slots.len();
        if self.len < cap {
            let idx = self.tail();
            self.slots[idx] = Some(value);
            self.len += 1;
            None
        } else {
            let idx = self.head;
            let evicted = self.slots[idx].take();
            self.slots[idx] = Some(value);
            self.head = (self.head + 1) % cap;
            evicted
        }
    }

    /// Pop from the front.
    pub fn pop(&mut self) -> Option<T> {
        if self.len == 0 {
            return None;
        }
        let cap = self.slots.len();
        let v = self.slots[self.head].take();
        self.head = (self.head + 1) % cap;
        self.len -= 1;
        v
    }

    pub fn front(&self) -> Option<&T> {
        if self.len == 0 {
            None
        } else {
            self.slots[self.head].as_ref()
        }
    }

    pub fn get(&self, i: usize) -> Option<&T> {
        if i >= self.len {
            return None;
        }
        let idx = (self.head + i) % self.slots.len();
        self.slots[idx].as_ref()
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> + '_ {
        (0..self.len).filter_map(move |i| self.get(i))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fifo_order() {
        let mut r: RingBuf<i32> = RingBuf::new(3);
        r.push(1);
        r.push(2);
        assert_eq!(r.pop(), Some(1));
        assert_eq!(r.front(), Some(&2));
    }

    #[test]
    fn overwrites_when_full() {
        let mut r: RingBuf<i32> = RingBuf::new(2);
        assert_eq!(r.push(1), None);
        assert_eq!(r.push(2), None);
        assert_eq!(r.push(3), Some(1)); // evicts oldest
        assert_eq!(r.iter().copied().collect::<Vec<_>>(), vec![2, 3]);
    }
}

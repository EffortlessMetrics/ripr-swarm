use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug, PartialEq)]
pub struct Snapshot { pub owner: usize }

pub struct Slot { inner: AtomicUsize }

impl Slot {
    pub fn new() -> Self { Slot { inner: AtomicUsize::new(0) } }

    pub fn claim(&self, id: usize) {
        self.inner.store(id, Ordering::Release);
    }

    pub fn snapshot(&self) -> Snapshot { Snapshot { owner: self.inner.load(Ordering::Acquire) } }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn claims_an_empty_slot() {
        let slot = Slot::new();
        slot.claim(7);
        assert_eq!(slot.snapshot(), Snapshot { owner: 7 });
    }
}

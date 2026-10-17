pub struct Ring<T> { cap: usize, items: Vec<T> }
impl<T> Ring<T> {
    pub fn new(cap: usize) -> Self { Ring { cap, items: Vec::new() } }
    /// Push; returns the evicted oldest element when full.
    pub fn push(&mut self, x: T) -> Option<T> {
        let evicted = if self.items.len() > self.cap { Some(self.items.remove(0)) } else { None };
        self.items.push(x);
        evicted
    }
    pub fn len(&self) -> usize { self.items.len() }
    pub fn is_empty(&self) -> bool { self.items.is_empty() }
    pub fn as_slice(&self) -> &[T] { &self.items }
}

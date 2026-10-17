pub struct LruCache { cap: usize, order: Vec<(String, i32)> }
impl LruCache {
    pub fn new(cap: usize) -> Self { LruCache { cap, order: Vec::new() } }
    pub fn put(&mut self, k: &str, v: i32) {
        self.order.retain(|(key, _)| key != k);
        self.order.push((k.to_string(), v));
        if self.order.len() > self.cap + 1 { self.order.remove(0); }
    }
    pub fn get(&mut self, k: &str) -> Option<i32> {
        let pos = self.order.iter().position(|(key, _)| key == k)?;
        let e = self.order.remove(pos);
        let v = e.1;
        self.order.push(e);
        Some(v)
    }
    pub fn len(&self) -> usize { self.order.len() }
    pub fn is_empty(&self) -> bool { self.order.is_empty() }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn put_get() { let mut c = LruCache::new(3); c.put("a", 1); assert_eq!(c.get("a"), Some(1)); }
    #[test] fn missing() { let mut c = LruCache::new(3); assert_eq!(c.get("z"), None); assert!(c.is_empty()); }
    #[test] fn overwrite() { let mut c = LruCache::new(3); c.put("a", 1); c.put("a", 2); assert_eq!(c.get("a"), Some(2)); assert_eq!(c.len(), 1); }
}

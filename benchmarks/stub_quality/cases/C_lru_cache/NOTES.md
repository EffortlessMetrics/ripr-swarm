1. Change: `LruCache::put` eviction threshold `len > self.cap` -> `len > self.cap + 1` (src/lib.rs); cache holds cap+1 entries.
2. Ideal test: `let mut c = LruCache::new(2); c.put("a",1); c.put("b",2); c.put("c",3); assert_eq!(c.get("a"), None); assert_eq!(c.len(), 2);`
3. Shape: &mut self, needs-constructor-args (cap), Option return, prior &mut calls, struct.

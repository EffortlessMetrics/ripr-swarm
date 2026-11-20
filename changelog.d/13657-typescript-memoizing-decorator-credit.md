<!-- section: Fixed -->
- TypeScript checks: a test of a method wrapped by a memoizing decorator (`if (!cache.has(k)) cache.set(k, original.call(this, k)); return cache.get(k);` over a private `new Map()`) now credits an exact assertion of an edit to the cached value instead of reporting no static path (#13657).

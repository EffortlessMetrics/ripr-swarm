<!-- section: Fixed -->
- Rust receiver resolution no longer treats every `Type::f()` initializer as returning `Type`, and no longer matches a type named within 96 bytes of a bracketed receiver. Related-test reasons, the test-grip direct-owner filter and transitive witnesses no longer infer `Site` from `Site::make_cache()`, and resolve `(cache).build()` from the receiver's own expression (#6303).

<!-- section: Fixed -->
- Rust receiver resolution no longer treats every `Type::f()` initializer as returning `Type`, and no longer matches a type named within 96 bytes of a bracketed receiver. Related-test reasons and transitive witnesses now resolve `Site::make_cache()` and `(cache).build()` from the receiver's own expression (#6303).

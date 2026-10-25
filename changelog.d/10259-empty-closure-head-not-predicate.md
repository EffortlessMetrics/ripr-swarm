<!-- section: Fixed -->
- Rust: an empty-parameter closure head (`unwrap_or_else(|| x)`, `ok_or_else(|| e)`, `move || work()`) no longer reads as a logical `||`, so a default-combinator rewrite is not classified as a `predicate_boundary` probe with boundary-input advice (#10259). A real `a || b` still reads as a predicate.

<!-- section: Fixed -->
- Rust: a test that builds a value with an indexed inherent constructor (`let m = Merge::new(..); format!("{m}")`) now reaches a changed `Display`/`Debug` `fmt` of that type; the constructor must be declared once and return exactly `Self` or the type, otherwise the receiver stays untyped (#8999, RIPR-SPEC-0297).

<!-- section: Fixed -->
- Rust assertions retain credit when a separate, manifest-declared fuzz binary
  mentions `assert_eq!` in macro arguments. In-scope shadowing and unresolved
  source composition still withhold credit, including whole-record and field-read
  assertions for changed constructed fields (#10191).

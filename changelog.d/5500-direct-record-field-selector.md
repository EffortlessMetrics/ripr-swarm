<!-- section: Added -->
- Rust match-arm analysis reads a direct record literal's Option field at the
  exact owner call and retains the existing same-assertion oracle requirement;
  ambiguous records, aliases and helper-returned mutable fixtures remain
  unresolved (#5500, #5502).

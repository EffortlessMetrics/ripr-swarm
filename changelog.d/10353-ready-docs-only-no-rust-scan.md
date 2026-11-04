<!-- section: Fixed -->
- `ripr check --mode ready` no longer indexes every workspace Rust file when
  the diff contains no analyzable Rust. A docs-only run now returns
  `no_behavioral_candidates` / complete, matching instant, draft, and fast,
  instead of `partial_with_limitations` about an unrelated unreadable file
  (#10353).

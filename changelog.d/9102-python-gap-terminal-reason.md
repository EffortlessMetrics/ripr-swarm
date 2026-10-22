<!-- section: Fixed -->
- A Python (or other non-Rust) finding with a complete gap no longer gets the
  generic "no seam in the finding's file binds this finding" or a stale-id
  message. `ripr check`, `ripr repair` and `ripr agent repair --seam-id` now
  state the precise terminal reason: the seam inventory behind the delegated
  repair loop covers Rust files only, and the printed gap, test node and verify
  command are the manual route. No repair start is offered (#9102).

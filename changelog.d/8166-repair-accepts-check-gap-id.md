<!-- section: Fixed -->
- `ripr repair <gap-id>` now accepts the Rust `Canonical gap:` ID that
  `ripr check` prints, instead of refusing it as "names no seam". The ID joins
  on the seam's file, owner, kind and changed expression; a non-matching or
  shared ID still refuses without starting an attempt, and the repair
  eligibility gate still applies (#8166 Slice C, resolver half).

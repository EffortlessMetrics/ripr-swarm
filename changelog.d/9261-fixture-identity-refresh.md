<!-- section: Added -->
- `cargo xtask fixtures refresh-identity <typescript|python>` re-derives an
  installed-journey fixture's file digests, snapshot commit/tree ids and
  scripted-corpus bindings after a dependency bump, using the same recipe
  rebuild `check-fixture-contracts` verifies with, so a security update to a
  fixture lockfile no longer needs hand-recomputed hashes (#9261).

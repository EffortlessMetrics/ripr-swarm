<!-- section: Fixed -->
- `ripr check` now credits a predicate that compares against a local whose live initializer spells the boundary literal (`val >= surrogate_start` after `let surrogate_start = 0xd800u32;`) instead of leaving infection unknown, and the first-run walk no longer reads the old `exposed/weak` legend prose as an `exposed` verdict under a `Static exposure: unknown` label (#11519).

<!-- section: Internal / CI -->
- `check-architecture` now ratchets inline consumer gap-state literals: production lines that spell `actionable`, `already_observed`, `internal_only`, `static_limitation` or `unknown` next to `gap_state` are counted per file against `policy/evidence-state-literals.txt`; new copies fail and the ceilings only fall (#2958).

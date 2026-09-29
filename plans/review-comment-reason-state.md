# Review-comment reasons follow canonical card state

Owning implementation issue: [#4912](https://github.com/EffortlessMetrics/ripr-swarm/issues/4912). Immutable starting main: `0dd150619632394437f3c167d274c61ef745bbf0`.

## Problem and authority

The real #1580 replay recorded in [#3074](https://github.com/EffortlessMetrics/ripr-swarm/issues/3074#issuecomment-5888783137) retained static-limitation cards whose `reason` encouraged a focused test while their typed guidance refused repair edits. This is a copy projection defect, not evidence of a repair route. Parent: #1693. Shared projection authority: #1663 and #1895.

## Narrow implementation

In `crates/ripr/src/output/review_comments.rs`, pass the already-derived canonical `gap_state` into `reason_for`. Only `actionable` may retain missing-discriminator or focused-test language. Other states must project their actual state without suggesting an edit. Do not derive readiness from missing values or prose. Preserve schema, typed guidance, identities, limits, commands and selection.

## Acceptance and proof

- Render the real existing static-limitation fixture and reject test-strengthening copy while retaining its typed refusal.
- Cover state precedence even when an optional missing discriminator exists.
- Preserve actionable missing-discriminator and actionable fallback reasons.
- Cover already-observed, internal-only and unknown states with no repair invitation.
- Use fallible regression assertions; no new source exception.

Proof: `cargo test -p ripr --lib review_comments -- --nocapture`, owning output-contract checks, `git diff --check`, and scoped rustfmt. Native inherited baseline and runtime proof are **not run** while the shared target lease belongs to another lane; source preparation is explicitly authorized, not runtime qualification. Root substantive source review precedes draft publication.

## Non-goals and remaining investigation

No new analyzer facts, test generation, consumer edits, canonical-route claim, or release claim. The #1981/#4833 investigation still needs the actual first refusing binder gate: null cards alone do not establish its cause, and mutable/nonliteral production RHS is not an established refusal. Keep that work with its existing owner. Rollback is this scoped projection change; no data migration.

# RIPR-SPEC-0094: observation_unverified Guard Generalization

Status: proposed

Owner: product / swarm

Created: 2026-06-13

Linked issues:

- #1216

Linked PRs:

- None yet

Support-tier impact:

- Narrows false-actionable over-claims for ReturnValue, FieldConstruction,
  SideEffect, and CallDeletion probes. Also closes a type-blind hole in the
  MatchArm guard from RIPR-SPEC-0093. Honesty improvement only; no tier change.
  Claim boundaries remain governed by the canonical ledger in
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- No new crates, binaries, dependencies, parsers, or LSP servers.
- Register this spec in `policy/doc-artifacts.toml`.
- No schema version bump. The `discriminate.summary` message for the unverified
  case changes text only (not a new JSON field).

## Behavior

When a probe belongs to a family where the changed sub-expression must be
directly witnessed by an assertion to justify `exposed` (ReturnValue,
FieldConstruction, SideEffect, CallDeletion, MatchArm), and no assertion
token references that changed expression, the discriminate stage is marked
`weak` and the finding is downgraded to `weakly_exposed`. The discriminate
summary contains `observation_unverified`. A token-match confirmation (any
assertion text contains an identifier token of length > 3 from the probe
expression — or, for MatchArm, from the variant tokens only) clears the
weakness and allows `exposed`.

## Problem

RIPR-SPEC-0093 introduced `arm_observation_unverified` for MatchArm only.
The same structural gap exists for four other families: ReturnValue,
FieldConstruction, SideEffect, and CallDeletion. Each can reach
`exposed`/1.00 via the `assertion_count == 1` escape hatch without any
assertion token referencing the changed sub-expression.

Additionally, inside the MatchArm guard, token_match was computed type-blind:
for `Mode::Frozen`, the tokens include `["Frozen", "Mode"]`. A sibling assertion
for `Mode::Warm` contains `"Mode"` (shared qualifier), which spuriously cleared
`observation_unverified` even though the test only exercises `Warm`.

## Fix

### Part A: Generalize the guard

Replace the `is_match_arm` single-family predicate with `needs_token_confirmation`
covering `{MatchArm, ReturnValue, FieldConstruction, SideEffect, CallDeletion}`.
Rename `arm_observation_unverified` to `observation_unverified`. Apply the
start-pessimistic / token_match-clears logic to all covered families. The
discriminate message when unverified becomes:
`"Discriminator unconfirmed: no assertion text references this probe's changed expression (observation_unverified)"`.

### Part B: Variant-scoped MatchArm token_match

Add `match_arm_variant_tokens(expression)` to extract only the variant tokens
(identifiers immediately after `::`) from the probe expression. For MatchArm,
`has_token_match` uses only these variant tokens, not the full token set. This
prevents the shared enum qualifier from confirming a sibling-arm assertion.

## Non-Goals

- Running mutations or dynamic analysis.
- Changing behavior for Predicate or ErrorPath probes.
- Bumping the JSON schema version.
- Producing `exposed` for SideEffect/CallDeletion when oracle strength is Medium.

## Required Evidence

Per-family fixture pairs (blind and confirmed) plus the MatchArm qualifier-blind lock.

## Inputs

- A diff changing a return expression, struct field initializer, side-effect call, or call-deletion.
- Related tests found by `find_related_tests`.

## Outputs

- `weakly_exposed` when `observation_unverified` fires (no token match).
- `exposed` when `token_match` fires for the specific changed sub-expression.
- `discriminate.summary` contains `"observation_unverified"` when unverified.

## Acceptance Examples

### ReturnValue blind (must downgrade)

```
probe family:  return_value
expression:    base * 2
test:          assert!(compute_score(3) > 0);
Before fix:    exposed / confidence 1.0
After fix:     weakly_exposed / confidence 0.92 / observation_unverified
```

### ReturnValue confirmed (must stay exposed)

```
probe family:  return_value
expression:    x * SCALE_FACTOR
test:          assert_eq!(compute_score(3), 3 * SCALE_FACTOR);
After fix:     exposed / confidence 1.0 (SCALE_FACTOR token_match fires)
```

### MatchArm sibling-qualifier blind (Part B, must downgrade)

```
probe family:  match_arm
expression:    Mode::Frozen => -1,
test:          assert_eq!(classify(Mode::Warm), 1);
Before fix:    exposed / confidence 1.0
After fix:     weakly_exposed / confidence 0.92 / observation_unverified
```

## Test Mapping

- `crates/ripr/src/analysis/classify/reveal.rs` unit tests for all new families.
- 9 new golden fixtures (see traceability.toml for the full list).
- `crates/ripr/src/analysis/classifier.rs` — 2 existing tests updated.

## Implementation Mapping

- `crates/ripr/src/analysis/classify/reveal.rs`:
  - `needs_token_confirmation(family)` new predicate.
  - `match_arm_variant_tokens(expression)` new helper for Part B.
  - `RevealAssertionAnalysis.observation_unverified` (renamed).
  - `analyze_related_assertions` applies guard to all covered families.
  - `assertion_matches_probe_detail` receives `match_arm_variants` param.
  - `build_discriminate_evidence` updated message.

## CI Proof

- `cargo xtask goldens check`
- `cargo test -p ripr`

## Metrics

- `observation_unverified_downgrades_non_match_arm_to_weakly_exposed`: each
  blind fixture produces `weakly_exposed` with `observation_unverified` in the
  discriminate summary.

No tier change required. Honesty narrowing within the existing `usable alpha` Rust exposure loop.

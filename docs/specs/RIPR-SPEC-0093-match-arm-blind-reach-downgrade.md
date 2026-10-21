# RIPR-SPEC-0093: Match-Arm Blind-Reach Downgrade

Status: proposed

Owner: product / swarm

Created: 2026-06-13

Linked proposal:

- None

Linked ADRs:

- None

Linked plan:

- None

Linked issues:

- #1198 — Arm-blind reach: match-arm mutation reported exposed/confidence-1.0 via a test that never observes the arm

Linked PRs:

- None yet

Support-tier impact:

- Narrows one false-actionable in the Rust static exposure loop: `MatchArm`
  probes whose only associated test has no identifier token from the arm's
  pattern or variant in its assertion text are downgraded from `exposed` to
  `weakly_exposed`. This is a honesty improvement, not a tier promotion or
  demotion. The `usable alpha` tier for "Rust static exposure loop" remains
  unchanged. Claim boundaries and tier governance remain governed by the
  canonical ledger in [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- No new crates, binaries, dependencies, parsers, or LSP servers.
- Register this spec in `policy/doc-artifacts.toml`.
- No schema version bump (`arm_observation_unverified` is a reason string inside
  the existing `discriminate.summary` field, not a new JSON field).

## Problem

A `MatchArm` probe represents a mutation on one arm of a `match` expression.
`ripr` reports it `exposed` (confidence 1.0) when:

1. A related test reaches the owner function (`calls_owner`).
2. That test has an exact-value oracle (`OracleKind::ExactValue`) which fires
   `oracle_matches_family(MatchArm, _)`.
3. `classify()` sees `discriminate.state == Yes` → `ExposureClass::Exposed`.

The false-actionable: `oracle_matches_family` for `MatchArm` accepts ANY
`ExactValue` assertion — including one that exercises a DIFFERENT arm. The
static data (call names, assertion text) does not encode which arm the test
actually enters, so the classification is wrong.

Concrete repro:

```rust
pub fn reason(x: Option<i32>) -> i32 {
    match x {
        Some(v) => v + 1,
        None => 0,   // ← probe target (new arm in diff)
    }
}

#[test]
fn only_some_arm() {
    assert_eq!(reason(Some(5)), 6);  // exercises Some arm only
}
```

Before this spec: ripr reports `None => 0` as `exposed / confidence 1.0`.
After: `weakly_exposed / arm_observation_unverified / confidence 0.92`.

## Behavior

For `ProbeFamily::MatchArm` probes:

1. For each assertion that matches the probe (via token, family, or escape
   hatch), record whether the assertion text contains any identifier token from
   the probe expression (`has_token_match`).
2. If the probe is a `MatchArm` AND no matched assertion has `has_token_match`,
   set `arm_observation_unverified = true`.
3. When `arm_observation_unverified` is true, `build_discriminate_evidence`
   returns `StageState::Weak` with summary
   `"Discriminator unconfirmed: no assertion text references this arm's pattern or variant (arm_observation_unverified)"`.
4. `classify()` sees `discriminate.state == Weak` → returns
   `ExposureClass::WeaklyExposed` instead of `ExposureClass::Exposed`.

Non-MatchArm probes are UNCHANGED. The escape-hatch behavior for other families
is also unchanged.

### Token-match exception

A `MatchArm` probe that has at least one matched assertion whose text contains
an identifier token from the probe expression (e.g. `Idle` in
`Status::Idle => 0,` matched by `assert_eq!(classify(Status::Idle), 0)`) stays
`exposed`. This confirms the test references the arm's specific pattern or
variant.

## Non-Goals

- Running mutations or dynamic analysis to confirm arm coverage.
- Distinguishing which specific arm a test enters at runtime.
- Changing the behavior for non-MatchArm probes.
- Bumping the JSON schema version (no new output fields).
- Changing `reachable_unrevealed` (the test IS reachable and HAS an oracle).

## Required Evidence

- `fixtures/match_arm_blind`: probe on `None => 0`, single-assertion test on
  `Some` arm → must produce `weakly_exposed`, `arm_observation_unverified`.
- `fixtures/match_arm_observer`: probe on `Status::Idle => 0`, test asserts
  `classify(Status::Idle)` (token `Idle` in assertion) → must stay `exposed`.

## Inputs

- A diff adding or modifying a `match` arm.
- Related tests found by `find_related_tests`.

## Outputs

- `weakly_exposed` (severity: warning) when `arm_observation_unverified`.
- `discriminate.summary` contains `arm_observation_unverified`.
- `recommended_next_step`: existing MatchArm guidance ("Strengthen the related
  assertion so it discriminates the changed behavior.").
- `missing`: `["No strong discriminator was detected"]`.

## Acceptance Examples

### arm-blind (must downgrade)

```
probe family:  match_arm
expression:    None => 0,
test:          assert_eq!(reason(Some(5)), 6);

Before fix:    exposed / confidence 1.0
After fix:     weakly_exposed / confidence 0.92 / arm_observation_unverified
```

### arm-observer (must stay exposed)

```
probe family:  match_arm
expression:    Status::Idle => 0,
test:          assert_eq!(classify(Status::Idle), 0);

Before fix:    exposed / confidence 1.0
After fix:     exposed / confidence 1.0   (unchanged — Idle token_match fires)
```

## Test Mapping

### Real terminal tuple projection (#1728)

The bounded derived-local witness also admits the retained
[`terminalize_proof` consumer correction](https://github.com/EffortlessMetrics/ripr/issues/1728#issuecomment-5747473563).
Its five ordered parameters, outer proof-request lookup, request sort/dedup,
borrowed request set, receipt iteration, matching sort, status validation,
`join={relation}` formatter and returned `Ok((status, reason, request_ids,
receipt_ids))` must match the frozen parsed grammar. Only the exact parser-owned
tuple-match node is a hole; the existing derived witness independently binds
its ordered immutable locals, exhaustive Boolean arms and candidate-current
changed literal. Lexer trivia is ignored; literal whitespace is preserved.

The supported oracle is one direct five-argument owner call followed by
`.unwrap()`, with a single immutable receipt literal and immutable
`BTreeMap::from` request maps. Duplicate keys, aliases, mutation and unknown
inputs refuse. The actual input maps determine request intersection and task
identity. Four executed top-level `assert_eq!` observations must pin the
returned status, exact reason containing the current relation, sorted/deduped
request IDs and receipt IDs. Sibling-only, diagnostic-only, no-op, uninvoked,
shadowed and post-return observations cannot refine the changed arm.

This extends static discrimination only for that grammar. Indirect serialized
caller projections, multiple-receipt oracle evaluation and arbitrary result
flow remain unsupported. The stage combiner retains ownership of the final
classification. Local or imported replacement macros, extern crates and
unknown wildcard imports refuse this terminal extension. The actual
`use crate::*` boundary reuses the existing indexed macro candidates and
parser-backed binding scan for `format`; every indexed file must remain
parser-owned, and a possible formatter replacement anywhere in the index
withholds the witness. A native public-API regression puts the retained owner
in an out-of-line module and rejects a formatter inherited from its crate root.
The same indexed value-name authority refuses declarations or named imports
that shadow `Some`, `Ok` or `None`; a compiling counterfeit `Some` can
otherwise erase the relation while preserving all four asserted fields.
The indirect serialized oracle remains separate retained acceptance under #1728.
No consumer rewrite, pin, suppression, threshold, release or
adoption follows. Roll back the terminal grammar while retaining its public
positive/negative regressions; the previously admitted precursor stays intact.

Test mapping:
`match_arm_tuple_derived_local::{real_terminal_request_derivation_and_reason_projection,
real_terminal_sibling_projections_remain_unverified,
real_terminal_owner_flow_controls_remain_unverified,
real_terminal_oracle_controls_remain_unverified}`, plus literal-token and
map-input controls in `tuple_match/derived_admission/terminal_flow.rs`.


- `crates/ripr/src/analysis/classify/reveal.rs` — unit tests for
  `assertion_matches_probe_detail` and `analyze_related_assertions`.
- `fixtures/match_arm_blind/` — golden fixture for the arm-blind case.
- `fixtures/match_arm_observer/` — golden fixture for the arm-observer case.

## Implementation Mapping

- `crates/ripr/src/analysis/classify/reveal.rs`:
  - `assertion_matches_probe_detail` (renamed from `assertion_matches_probe`,
    returns `(matched, has_token_match)`).
  - `RevealAssertionAnalysis.arm_observation_unverified`.
  - `analyze_related_assertions` populates `arm_observation_unverified`.
  - `build_discriminate_evidence` checks `arm_observation_unverified`.

## CI Proof

- `cargo xtask fixtures match_arm_blind`
- `cargo xtask fixtures match_arm_observer`
- `cargo xtask goldens check`
- `cargo test -p ripr`

## Metrics

- `match_arm_blind_downgrades_to_weakly_exposed`: fixture `match_arm_blind`
  produces `weakly_exposed` with `arm_observation_unverified` in the
  discriminate summary (validated by `cargo xtask fixtures match_arm_blind`).

No tier change required. This is a honesty narrowing of a known false-actionable
within the existing `usable alpha` Rust exposure loop.

## Failure Modes

- A MatchArm probe with no filtered tokens (e.g. `None => 0`) will always
  produce `weakly_exposed` regardless of what the test does. This is acceptable:
  we cannot confirm arm observation when the static data has no arm-specific
  tokens. Callers with a genuine observer should use a named constant or variant
  that produces a token in the probe expression.
- A MatchArm probe that shares enum-type tokens across arms (e.g. `Status::X`
  and `Status::Y` share `Status`) may have spurious token_match from the shared
  prefix. This is a known limitation; the net direction is still fail-closed
  (fewer false-actables, not fewer true-positives).

Opaque item/statement expansions anywhere in the indexed namespace also refuse
terminal admission: a macro can emit an inherited `Some` without spelling that
name at its invocation. Expression expansions stay confined to their expression
scope; indexed standard macro authority and ordinary Option constructors retain
the existing paths. The generated-constructor honesty fixture and public API
regression retain the compiling wrong-arm counterexample from #7836 review.

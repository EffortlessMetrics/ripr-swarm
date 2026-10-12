# RIPR-SPEC-0104: TypeScript Oracle Seam-Family Match (Assertion-Level)

Status: accepted

Owner: product / swarm

Created: 2026-06-14

Linked issues:

- #1246

Linked PRs:

- None yet

Support-tier impact:

- Honesty fix for TypeScript preview output: a `ReturnValue` or `Predicate` seam
  that has only a cross-family Strong oracle (e.g. `.toThrow(DiscountError)`,
  `ExactErrorVariant`) no longer classifies as `Exposed / strong_oracle_observed`.
  It now correctly classifies as `weakly_exposed` with an actionable repair card.
  This is a withdraw-only fix â€” it credits nothing and cannot raise any grip.
  The exposure class moves `Exposed â†’ WeaklyExposed` only for the mis-credited case;
  seams with a genuine family-matching Strong oracle are unaffected.
  Claim boundaries and tier labels remain governed by the canonical ledger in
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Two new `pub(crate)` functions in `classifier.rs`:
  `ts_oracle_kind_matches_seam` (the mapping table) and
  `strongest_family_matching_oracle` (assertion-level aggregation).
- `classify_change` now computes `strongest_strength`/`strongest_kind` by iterating
  assertions at the assertion level (via `related_candidates`), filtered through
  `ts_oracle_kind_matches_seam`, instead of collapsing at the test level via `related`.
- `SideEffect` / `CallDeletion` families are NOT filtered (all oracle kinds admitted)
  so that RIPR-SPEC-0098 observation guard continues to fire in the
  `else if strongest_strength >= Strong.rank()` arm.
- No `schema_version` bump. No new exposure class or vocab.
- Static-language clean: all output uses allowed vocabulary only
  (`exposed`, `weakly_exposed`, etc.). No `killed`/`survived`/`untested`/`proven`/`adequate`.
- Register this spec in `policy/doc-artifacts.toml` and `docs/specs/README.md`.

## Problem

In `classifier.rs`, `strongest_strength` and `strongest_kind` were computed as the
maximum over `related: Vec<RelatedTest>`, where each `RelatedTest` carries a single
`oracle_kind` collapsed from the test's overall-strongest assertion by
`strongest_assertion()` in `related_tests.rs`.

This collapsed oracle_kind may be wrong-family for the changed seam. Example:

A function `applyDiscount` has:
- An error-path test: `expect(() => applyDiscount(-1, 'gold')).toThrow(DiscountError)`
  â†’ oracle_kind `ExactErrorVariant`, strength `Strong`.
- A return-value test: `expect(applyDiscount(100, 'gold')).toBeGreaterThan(0)`
  â†’ oracle_kind `RelationalCheck`, strength `Weak`.

A diff changes `0.95 â†’ 0.90` on the line `return amount * 0.95` (`ReturnValue` seam).

Before this fix: the test-level max is `ExactErrorVariant/Strong`, so
`strongest_strength >= Strong.rank()` is true â†’ `observation_confirmed` passes
(RIPR-SPEC-0098 guard is not scoped to ReturnValue family) â†’ `Exposed / strong_oracle_observed /
oracle_kind: exact_error_variant`. This is a FAKE-CLEAN: the error-variant discriminator
does not observe the changed return multiplier.

## Behavior

### Pure helper `ts_oracle_kind_matches_seam`

Added to `classifier.rs`. Maps `(OracleKind, ProbeFamily)` â†’ `bool`.

**Excluded oracle kinds per family:**

| ProbeFamily | Excluded oracle kinds (cross-domain honesty boundary) |
|---|---|
| `ErrorPath` | `ExactValue`, `RelationalCheck`, `MockExpectation` |
| `ReturnValue`, `Predicate`, `FieldConstruction`, `MatchArm` | `ExactErrorVariant`, `BroadError`, `MockExpectation` |
| `SideEffect`, `CallDeletion` | *(none â€” all admitted; RIPR-SPEC-0098 handles these)* |
| `StaticUnknown` | *(none â€” fail-open when family is genuinely unknown)* |

Key design decisions:

- Only semantically cross-domain Strong oracle kinds are excluded (e.g. `ExactErrorVariant`
  on a `ReturnValue` seam). Weak/absent kinds (`SmokeOnly`, `Unknown`) are admitted for
  all concrete families â€” their rank prevents them from triggering `Exposed` promotion
  and their admission preserves accurate per-kind messaging in `weak_oracle_missing_summary`.
- `SideEffect` / `CallDeletion`: all oracle kinds admitted. RIPR-SPEC-0098
  (`ts_changed_value_is_observed`) already handles the case where a value-shaped
  `ExactValue` Strong assertion observes the owner return value (not the call effect).
  The two specs compose: SPEC-0104 handles cross-domain valueâ†”error exclusion;
  SPEC-0098 handles same-domain but wrong-target SideEffect cases.
- `StaticUnknown` is fail-open: the probe shape is genuinely unclassified, so
  blocking any oracle kind would be over-correction.

### Assertion-level aggregation `strongest_family_matching_oracle`

Added to `classifier.rs`. Iterates `related_candidates: Vec<TypeScriptRelatedCandidate>`,
which carry `.test.assertions` (the full per-test assertion slice). For each
oracle-eligible candidate, it filters assertions by `ts_oracle_kind_matches_seam` and
computes the max `(oracle_strength.rank(), oracle_kind)` over the matching assertions.

This is the anti-over-correction guard (control 4): a test that asserts BOTH
`.toThrow(DiscountError)` (wrong-family for ReturnValue) AND `.toBe(90)` (family-matching
for ReturnValue) is NOT dropped. The test-level filter would drop the whole test because
its overall-strongest assertion is wrong-family. The assertion-level filter retains the
`.toBe(90)` assertion and the seam correctly stays `Exposed`.

### Fail-closed downgrade (reusing existing paths)

`classify_change` now calls `strongest_family_matching_oracle` instead of iterating
`related` for `strongest_strength` / `strongest_kind`. When no family-matching assertion
of Strong rank exists:

- `strongest_strength < Strong.rank()` â†’ falls through to the `else` arm â†’
  `WeaklyExposed` + `weak_oracle_missing_summary` + `missing_discriminators` repair card.
  No new class or vocab.
- The RIPR-SPEC-0098 observation-guard path (`else if strongest_strength >= Strong.rank()
  && !observation_confirmed`) is reached only when a family-matching Strong assertion
  EXISTS but the observation guard fails (SideEffect case) â€” the two specs compose.

## Required Evidence

All tests in `crates/ripr/src/analysis/language/typescript/tests.rs`:

1. **REPRO** `spec_0104_repro_cross_family_error_oracle_does_not_promote_return_value_seam`:
   Gold return-value change; two separate tests: `.toThrow(DiscountError)` (Strong,
   ExactErrorVariant) + `.toBeGreaterThan(0)` (Weak, RelationalCheck) â†’ MUST become
   `weakly_exposed`. The headline fix.

2. **MUST-NOT-OVER-CORRECT** `spec_0104_no_over_correct_return_value_with_exact_value_stays_exposed`:
   Return-value change with `expect(fn(...)).toBe(90)` (ExactValue, Strong) â†’
   STAYS `exposed`. ExactValue matches ReturnValue.

3. **MUST-NOT-OVER-CORRECT** `spec_0104_no_over_correct_error_path_with_exact_error_variant_stays_exposed`:
   Error-throw change with `expect(()=>fn(-1)).toThrow('Invalid amount')` (ExactErrorVariant,
   Strong) â†’ STAYS `exposed`. ExactErrorVariant matches ErrorPath.

4. **SINGLE-TEST-BOTH-ASSERTIONS** `spec_0104_single_test_both_assertions_retains_matching_family_assertion_stays_exposed`:
   ONE test with BOTH `.toThrow(DiscountError)` (ExactErrorVariant, Strong, wrong-family
   for ReturnValue) AND `.toBe(90)` (ExactValue, Strong, matches ReturnValue); diff changes
   the gold return value â†’ MUST STAY `exposed`. Proves the fix operates at the assertion
   level, not the test level: the test is NOT dropped because of its toThrow assertion.

5. **MAPPING TABLE** `spec_0104_ts_oracle_kind_matches_seam_mapping_table`:
   Unit test for `ts_oracle_kind_matches_seam` covering all (ProbeFamily, OracleKind) pairs
   documented in the mapping table, including the primary fix assertion
   (`ExactErrorVariant` rejected from value-family seams).

## Non-Goals

- Does NOT touch `ts_changed_value_is_observed` (#1235 / RIPR-SPEC-0098). The SideEffect
  observation guard is a different layer.
- Does NOT add a new exposure class or any new output vocab.
- Does NOT change `related: Vec<RelatedTest>` (used for display and mock-payload lookup).
- Does NOT bump `schema_version`.
- Does NOT bump crate version, publish, or touch release workflows.
- TypeScript preview-tier status is unchanged.
- Static-language clean: no `killed`/`survived`/`untested`/`proven`/`adequate` in output.

## Acceptance Examples

### Before (fake-clean â€” ExactErrorVariant credits a ReturnValue seam)

```
classification: exposed
oracle_kind: exact_error_variant
actionability_category: strong_oracle_observed
why_not_actionable: related Jest/Vitest evidence already has a strong exact oracle
```

### After (correct â€” weakly_exposed / actionable repair card)

```
classification: weakly_exposed
actionability_category: incomplete_repair_packet
missing_discriminator: return value == amount * 0.90
```

### Controls unchanged

Control 2 (return + exact value):

```
classification: exposed
oracle_kind: exact_value
```

Control 3 (error + toThrow exact payload):

```
classification: exposed
oracle_kind: exact_error_variant
```

Control 4 (single test, both assertions â€” assertion-level filter keeps matching oracle):

```
classification: exposed
oracle_kind: exact_value
```

## Test Mapping

| Test | Fixture |
|---|---|
| `spec_0104_repro_cross_family_error_oracle_does_not_promote_return_value_seam` | 1 â€” headline fix: ExactErrorVariant on ReturnValue seam â†’ weakly_exposed |
| `spec_0104_no_over_correct_return_value_with_exact_value_stays_exposed` | 2 â€” must-not-over-correct: ExactValue on ReturnValue stays exposed |
| `spec_0104_no_over_correct_error_path_with_exact_error_variant_stays_exposed` | 3 â€” must-not-over-correct: ExactErrorVariant on ErrorPath stays exposed |
| `spec_0104_single_test_both_assertions_retains_matching_family_assertion_stays_exposed` | 4 â€” over-correction guard: assertion-level filter keeps family-matching assertion |
| `spec_0104_ts_oracle_kind_matches_seam_mapping_table` | 5 â€” mapping table parity |

## Implementation Mapping

| Behavior | Code location |
|---|---|
| `ts_oracle_kind_matches_seam` (mapping table, pure) | `crates/ripr/src/analysis/language/typescript/classifier.rs` |
| `strongest_family_matching_oracle` (assertion-level max) | `crates/ripr/src/analysis/language/typescript/classifier.rs` |
| `classify_change` uses assertion-level aggregation | `crates/ripr/src/analysis/language/typescript/classifier.rs` |
| 5 unit tests (4 controls + mapping table) | `crates/ripr/src/analysis/language/typescript/tests.rs` |
| Spec registration | `policy/doc-artifacts.toml`, `docs/specs/README.md` |
| Traceability | `.ripr/traceability.toml` |

## Later Amendment

RIPR-SPEC-0243 (2026-10-04) states the aggregation this spec feeds. The
class reads the strongest family-matched assertion, and ties keep the
first one seen. `related_tests[]` oracle fields stay each test's own
strongest assertion and are not family-filtered.

## Metrics

- `typescript_oracle_seam_family_honesty`: a `ReturnValue` seam with only a
  cross-family Strong oracle (`ExactErrorVariant`) classifies as `weakly_exposed`
  (control 1); control 2â€“4 remain `exposed` (no over-correction).

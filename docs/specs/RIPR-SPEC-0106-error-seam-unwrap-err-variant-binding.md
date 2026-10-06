# RIPR-SPEC-0106: Error-Seam unwrap_err / expect_err Variant Binding

Status: accepted

Owner: product / swarm

Created: 2026-06-14

Linked issues:

- #1168
- #6695 (`ok_or(Variant)?` propagation, Part C)

Linked PRs:

- None yet

Support-tier impact:

- Honesty fix for Rust primary-language output: `error_path` seams backed by a
  `let err = f().unwrap_err(); assert_eq!(err, MyError::Variant)` pattern now
  correctly report `exposed` with a `strong` discriminator instead of
  `weakly_exposed`. Grip may only RISE when the assertion structurally pins the
  changed seam's exact variant. Sibling-variant, generic-error, and
  unprovable-variant assertions remain `weakly_gripped` (fail-closed).
  Claim boundaries and tier labels remain governed by the canonical ledger in
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- No new crates, binaries, or LSP servers.
- New `pub(crate)` functions `unwrap_err_bound_variables`,
  `is_unwrap_err_bound_error_assertion`, `contains_named_enum_variant` in
  `crates/ripr/src/analysis/extract/oracles/patterns.rs` and
  `crates/ripr/src/analysis/extract/oracles/scan.rs`.
- New `pub(in crate::analysis)` re-exports in
  `crates/ripr/src/analysis/classify/mod.rs` for `enum_variant_values` and
  `exact_error_variant`.
- `assertion_matches_probe_detail` in `classify/reveal.rs` gains an
  `error_path_variant: Option<&str>` parameter; all callers updated.
- `discriminate_evidence` in `test_grip_evidence.rs` delegates to new
  `oracle_discriminates_seam` (variant-aware) instead of plain
  `oracle_kind_matches_seam`.
- No `schema_version` bump. The `exposed` / `strong` shape is an existing
  valid output contract.
- Register this spec in `policy/doc-artifacts.toml` and `docs/specs/README.md`.

## Problem

For an `ErrorVariant` seam, the common test pattern is:

```rust
let err = f(-1).unwrap_err();
assert_eq!(err, MyError::Variant);
```

Before this PR, the `assert_eq!(err, MyError::Variant)` assertion was lexed as
`OracleKind::ExactValue` (not `ExactErrorVariant`) because the line does not
contain `Err(`. This caused:

- `error_path` seams to report `weakly_exposed` even when the test structurally
  pins the exact error variant — a false-weak rating.
- The seam to show a misleading "missing discriminator" warning when the
  discriminator is actually present.

### The defect root

`is_exact_error_variant_assertion` in `patterns.rs` required `Err(` in the
assertion line. A `unwrap_err()` binding eliminates the `Err(` wrapper:

```rust
// asserted directly (ExactErrorVariant via Err( — old path works):
assert_eq!(result, Err(MyError::Variant));

// asserted via binding (ExactValue erroneously — this PR fixes):
let err = result.unwrap_err();
assert_eq!(err, MyError::Variant);
```

## Behavior

### Part A — Recognition (both paths)

Before classifying assertions in a test body, perform a pre-pass to collect
`unwrap_err_bound_variables`: variables bound as
`let <var> [: <type>] = <expr>.unwrap_err()` or
`let <var> [: <type>] = <expr>.expect_err("…")`.

When an assertion references one of these bound variables AND contains a named
enum-variant token (`SomePath::Variant` with uppercase last component), upgrade
the oracle from `ExactValue` to `ExactErrorVariant` / `Strong`. This upgrade
applies in BOTH the lexical path (`extract_assertions` in `scan.rs`) and the
rust-analyzer path (`extract_parser_oracles` in `ra.rs`).

FAIL-CLOSED rule: when the binding is present but the assertion does NOT
contain a named enum-variant token (e.g. `assert!(err.to_string().contains(…))`),
do NOT upgrade. The classification stays at its original kind.

### Part B — Variant binding (over-credit guard, both paths)

For `ErrorPath` probes, an `ExactErrorVariant` assertion only credits the seam
when it pins the probe's SPECIFIC variant token. A sibling-variant assertion
(`CalcError::Negative`) must NOT credit a `CalcError::TooLarge` seam, even
though both share the `CalcError` qualifier token.

Implementation:

- **Diff-mode** (`classify/reveal.rs`): `error_path_variant_path` extracts the
  probe's required error path; its post-`::` uppercase component is the
  variant token. In
  `assertion_matches_probe_detail`, when the probe family is `ErrorPath` and the
  assertion kind is `ExactErrorVariant` and `error_path_variant` is `Some`, the
  match is restricted to assertions whose text contains the specific variant
  token. Without a parseable variant, fall through to the standard match.

- **Repo-exposure** (`test_grip_evidence.rs`): `oracle_discriminates_seam`
  replaces the plain `oracle_kind_matches_seam` call for grip grading.
  For `ErrorVariant` seams, it additionally calls
  `error_variant_oracle_matches_seam_variant`, which parses both the
  `RequiredDiscriminator::ErrorVariant { variant }` on the seam and the
  oracle assertion text and rejects a mismatch.

### Part C — `ok_or(Variant)?` returns the variant from the owner (#6695)

A changed statement `let d = helper(c).ok_or(Type::Variant)?;` (or
`.ok_or_else(|| Type::Variant)?`) returns `Err(Type::Variant)` from the
owner when the helper yields `None`. The changed error's identity is that
variant, exactly as for `return Err(Type::Variant)`:

- `text::question_mark_error_variant` reads the variant only when the
  line holds exactly one `.ok_or(`/`.ok_or_else(` call at delimiter depth
  zero, its `?` directly follows the call and ends the statement, no `|`
  sits at depth zero before it, and the argument is one qualified variant
  path (upper-case final segment, optional single payload) — for
  `ok_or_else`, the body of a parameterless `||` thunk. Every other shape
  (a call or `.into()` argument, a bare imported variant name, a second
  conversion on the line, `?.field`, a closure head) reads `None`.
- The ErrorPath flow sink becomes `Result::Err(Type::Variant)` only when
  `flow::question_mark_returns_from_owner` confirms, over the masked
  owner body, that the line is the changed expression and that no
  enclosing delimiter between the owner's body brace and the line opens
  a closure (`|`), an `async`/`try`/`move`/`gen` block, a nested item
  (`fn`/`impl`/`mod`/..), a macro body (`!`), or a call argument list —
  in each of those the `?` returns from something other than the owner.
- With that sink, the propagation witness edge is established and
  complete (the `||` thunk is the argument's own constant, not an opaque
  path), so an exact `Err(Type::Variant)` pin on the owner call can read
  `exposed`.
- Part B applies to the same variant in both modes through one identity
  owner, `text::changed_error_variant` (`exact_error_variant`, falling
  back to `question_mark_error_variant`). Diff mode: reveal's
  `error_path_variant_path` reads it, so a test pinning a sibling variant
  (`Err(Type::Other)`) does not confirm the line. Repo mode:
  `seam_inventory::required_discriminator_for` stores it as the
  `ErrorVariant` seam's identity, and
  `guarded_result_oracle_matches_seam_variant` compares a return-value
  seam's guarded pins against it, so a sibling pin does not discriminate
  the seam there either.
- Part B's sibling gate covers every assertion kind, not only
  `ExactErrorVariant`: an assertion that spells the changed error's enum
  only through sibling variant paths (an `exact_value`
  `assert!(matches!(e, Type::Other))` inside a match arm) shares just the
  enum qualifier with the changed line. It does not match an `error_path`
  probe and confirms no variant-carrying family
  (`reveal::names_only_sibling_variants`, PR #6786 review).

On an `.ok_or_else(|| Type::Variant)?` line the probe extractor also
emits a `predicate` probe, because it reads the parameterless closure head
`||` as a logical-OR operator. That probe predates this part. It keeps
reading `infection_unknown` ("no literal boundary was visible"), which is
a non-actionable unknown, not a gap, and this part does not change it.

Part B also covers the turbofish constructor: `exact_error_variant`
reads `Err::<T, E>(Type::Variant)` like `Err(Type::Variant)`, and the
witness compares error identities with the turbofish removed. Before,
that spelling carried no variant, so a sibling-variant pin from another
test of the same owner could confirm it and read `exposed`.

A related honesty fix: the witness's opaque-path check names an FFI
boundary only for an `ffi` identifier word (`std::ffi::CStr`, `ffi_call`),
not for the letters `ffi` inside a word, which refused every witness for
variants such as `PayError::Insufficient` (#6673).

## Non-Goals

- Does NOT recognize `expect_err` in a middle position (only at the end of the
  binding expression).
- Does NOT handle indirect rebinding (`let e = err; assert_eq!(e, …)`).
- Does NOT add any Rust-analyzer cross-function flow; this remains lexical.
- Does NOT change `oracle_kind_matches_seam_kind` (unchanged single source of
  truth for seam-kind ↔ oracle-kind matching used by grip grading).
- Does NOT bump `schema_version`.
- Does NOT bump crate version, publish, or touch release workflows.
- Static-language clean: all new code and output uses allowed vocabulary only.

## Required Evidence

### Fixture 1 — POSITIVE (unwrap_err_variant_positive)

- Changed seam: `return Err(CalcError::Negative)`
- Test: `let err = compute(-1).unwrap_err(); assert_eq!(err, CalcError::Negative);`
- Expected: `error_path` → `exposed`, `discriminator yes`, `exact_error_variant`

### Fixture 2 — SIBLING-VARIANT (unwrap_err_sibling_variant)

- Two error returns: `CalcError::Negative` and `CalcError::TooLarge`
- Changed seam: `TooLarge`
- Test: only pins `CalcError::Negative`
- Expected: `error_path` for `TooLarge` → NOT `exposed` (at most `weakly_exposed`)

### Fixture 3 — GENERIC (unwrap_err_generic_is_err)

- Changed seam: `return Err(CalcError::Negative)`
- Test: `let err = compute(-1).unwrap_err(); assert!(err.to_string().contains("error"));`
- Expected: `error_path` → `weakly_exposed` (generic assertion; no variant token)

### Fixture 4 — SUCCESS-PATH NO REGRESSION (existing: weak_error_oracle)

- Test uses `assert!(authenticate("").is_err())` with no `unwrap_err()` binding
- Expected: `error_path` remains `weakly_exposed` (no regression from this PR)

### Fixture 5 — BOXED WRAPPER DOWNCAST WITNESS / TYPED BINDING LIMITATION (error_variant_boxed_wrapper_downcast_witness, #3700)

Final semantics (#3700 design decision): whether a wrapper error conversion
(`callee(..).map_err(..)` over `Box<dyn Error>`) faithfully carries the
converted callee's error variant is **not statically establishable** by
lexical analysis. Three review rounds of lexical binding heuristics each
produced new over/under-credit edge cases, so the fail-closed end state the
issue sanctions is the typed static limitation
(`wrapper_error_binding_unresolved`), not a binding heuristic.

- Positive (parseable-variant path, unchanged main behavior): the typed
  `try_parse_summary` seam `return Err(ParseSummaryError::MalformedSource);`
  classifies `exposed` with `exact_error_variant` / `strong` — credited only
  through the pre-existing variant-bound path, with no wrapper heuristics.
- Wrapper `map_err(Into::into)` seams with asserting observers classify
  `weakly_exposed`, never `exposed`; lexical confirmation is refused by
  construction (every overlap between the seam expression and witness text is
  token coincidence), and the finding carries
  `static_limit_kind: wrapper_error_binding_unresolved` with limitation
  evidence naming the unresolved `Into`/`From`-through-`Box` edge. The downcast
  witness and the typed sibling test are still listed as related, but the
  emitted guidance no longer prescribes an assertion the suite may already
  contain.
- Asserting fail-closed companions in the same input stay `weakly_exposed`: a
  wrong-sibling downcast witness, an unrelated-enum downcast witness, a broad
  `is_err()`-only observer, and a stringified conversion
  (`map_err(|error| error.to_string().into())`).
- The ignored `matches!` result in `theme_summary` is not an asserting observer.
  Its wrapper `error_path` and `return_value` findings classify
  `reachable_unrevealed`, retain a `no_assertion` consumer and `unknown` / `none`
  oracle metadata, and recommend adding an assertion. Secondary missing text
  may retain the unresolved wrapper-binding context while the optional
  `static_limit_kind` and `static_limitation` fields are absent.
- Companion fixtures: `fixtures/error_variant_boxed_wrapper_fail_closed`
  (all-weak source for the wrong-sibling and unrelated-enum shapes) and
  `fixtures/error_variant_wrapper_{callee_only_pin,foreign_pin,
  wrong_receiver_pin}`. All are pinned under RIPR-SPEC-0108 with
  `must_emit_limitation` (`wrapper_error_binding_unresolved`),
  `must_not_promote`, and `maximum_class: weakly_exposed`.
- Follow-up: modeling `Into`/`From`-through-`Box` conversions so a faithful
  typed conversion can be credited is tracked as a follow-up slice (#1617
  family); until then the limitation is the honest output.

## Unit Tests

Tests in `crates/ripr/src/analysis/extract/oracles/` and
`crates/ripr/src/analysis/classify/reveal.rs`:

1. `unwrap_err_binding_recognized_and_variable_collected` — `unwrap_err_bound_variables`
   collects the bound variable name.
2. `is_unwrap_err_bound_error_assertion_upgrades_named_variant` — upgrade fires
   when assertion references bound var AND contains enum variant token.
3. `generic_assertion_on_bound_var_not_upgraded` — assertion without variant
   token stays `ExactValue`.
4. `sibling_variant_assertion_does_not_match_tool_large_probe` —
   `assertion_matches_probe_detail` with `error_path_variant = Some("TooLarge")`
   and assertion text containing only `Negative` → `(false, false)`.

## Test Mapping

| Test | Fixture |
|---|---|
| `unwrap_err_binding_recognized_and_variable_collected` | Part A recognition |
| `is_unwrap_err_bound_error_assertion_upgrades_named_variant` | Fixture 1 positive |
| `generic_assertion_on_bound_var_not_upgraded` | Fixture 3 generic |
| `sibling_variant_assertion_does_not_match_tool_large_probe` | Fixture 2 sibling |
| `question_mark_error_variant_reads_ok_or_and_ok_or_else` | Part C recognition |
| `question_mark_error_variant_refuses_every_other_shape` | Part C fail-closed shapes |
| `question_mark_ok_or_in_the_owner_body_is_a_complete_error_witness` | Part C propagation |
| `question_mark_ok_or_inside_a_closure_or_async_block_is_not_owner_propagation` | Part C enclosure negatives |
| `ffi_boundary_is_an_identifier_word_not_a_substring` | Part C opaque-path fix |
| `exact_error_variant_reads_turbofish_and_qualified_constructors` | Part B turbofish binding |

## Acceptance Examples

### Before (incorrect — weakly_exposed even with exact variant test)

```
Probe
  family: error_path
  delta:  value

Static exposure
  weakly_exposed (warning, confidence 0.92)

Evidence
  - discriminator weak: Medium oracle found: property or partial structural assertion
  - related test tests/errors.rs:4 uses medium exact error variant oracle: assert_eq!(err, CalcError::Negative);
```

### After (correct — exposed, strong discriminator)

```
Probe
  family: error_path
  delta:  value

Static exposure
  exposed (info, confidence 1.00)

Evidence
  - discriminator yes: Strong oracle found: exact error variant assertion
  - related test tests/errors.rs:4 uses strong exact error variant oracle: assert_eq!(err, CalcError::Negative);
```

## Implementation Mapping

| Behavior | Code location |
|---|---|
| `unwrap_err_bound_variables` body pre-pass | `crates/ripr/src/analysis/extract/oracles/scan.rs` |
| `is_unwrap_err_bound_error_assertion` upgrade gate | `crates/ripr/src/analysis/extract/oracles/patterns.rs` |
| `contains_named_enum_variant` token check | `crates/ripr/src/analysis/extract/oracles/patterns.rs` |
| Lexical path upgrade (`extract_assertions`) | `crates/ripr/src/analysis/extract/oracles/scan.rs` |
| RA path upgrade (`extract_parser_oracles`) | `crates/ripr/src/analysis/syntax/ra.rs` |
| Sibling-variant guard — diff-mode | `crates/ripr/src/analysis/classify/reveal.rs` |
| Sibling-variant guard — repo-exposure | `crates/ripr/src/analysis/test_grip_evidence.rs` |
| `enum_variant_values`, `exact_error_variant` re-exported | `crates/ripr/src/analysis/classify/mod.rs` |
| Part C `question_mark_error_variant` | `crates/ripr/src/analysis/classify/text/error_variant.rs` |
| Part C owner-enclosure gate and error sink | `crates/ripr/src/analysis/classify/flow.rs` |
| Part C established witness edge, FFI word check | `crates/ripr/src/analysis/classify/propagation_witness.rs` |
| Spec registration | `policy/doc-artifacts.toml`, `docs/specs/README.md` |
| Traceability | `.ripr/traceability.toml` |

## Metrics

- `unwrap_err_variant_grip_raise`: `error_path` seam with exact variant
  `unwrap_err` binding → `exposed` (fixture 1).
- `sibling_variant_no_credit`: `error_path` seam for `TooLarge` not credited
  by a `Negative`-only test (fixture 2, fail-closed guard).

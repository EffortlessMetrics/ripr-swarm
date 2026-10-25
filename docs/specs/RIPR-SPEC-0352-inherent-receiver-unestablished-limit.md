# RIPR-SPEC-0352: An inherent receiver blocked only by an unverifiable assertion macro is a named limitation, not a gap

Status: proposed

Owner: product-swarm

Created: 2026-10-24

Linked proposal:

- None yet

Linked ADRs:

- None yet

Linked plan:

- release-0.11 trustworthy-verdicts scoreboard

Linked issues:

- #9611 (inherent-receiver identity), #10122 (unplaced workspace receiver)

Linked PRs:

- None yet

Support-tier impact:

- None.
- This narrows when Rust findings claim a gap. It does not promote a
  language, surface, or gate.

Policy impact:

- Register this spec in `policy/doc-artifacts.toml` and
  `.ripr/traceability.toml`.
- Add one `static_limit_kind` value (`rust_inherent_receiver_unestablished`)
  to `policy/output_contracts.txt`, `docs/OUTPUT_SCHEMA.md`, and
  `docs/STATIC_LIMITS.md`. The JSON schema version does not change; the value
  is additive and the RIPR-SPEC-0240 `gap_evidence_unresolved` stop reason is
  reused.

## Problem

An inherent method owner relates to a test as `direct_owner_call` only when the
receiver's type is established (#9611). A call written inside a standard
assertion, `std::assert_eq!(Widget::new().read(), 4)`, counts only when the
assertion macro binding is verified, and the verification needs target-root
placement: which Cargo target compiles the test file, and therefore which
`assert_eq!` that target sees. An unresolved `include!` or `#[path]` anywhere in
the workspace (a build-script output such as
`include!(concat!(env!("OUT_DIR"), "/plain.rs"))`) may mount any file into
another crate, so placement is unavailable and the binding cannot be verified.

The refusal then dropped the relation to `weak_token_substring` and the finding
to `weakly_exposed` or `reachable_unrevealed`: a certain "no test notices this"
claim, and an actionable repair signal, resting on a receiver that was never
shown to differ. The call resolves if the macro can be trusted, and whether it
can be trusted is exactly what ripr cannot read.

## Trigger

All of these hold, and any doubt keeps today's behavior:

1. The finding is a Rust gap (`weakly_exposed` or `reachable_unrevealed`) with
   no earlier `static_limit_kind`.
2. The owner is an inherent method with a receiver
   (`requires_inherent_receiver_identity`).
3. Some test that is one of the finding's own related tests (same name and
   file as a related row) and is not `#[ignore]` has an owner-call relation
   that is not `direct_owner_call` with the standard-root check enforced, and
   **is** `direct_owner_call` with only the placement input of that check
   lifted. The comparison runs the complete owner-call relation, so a wrong
   or message-only receiver, a shadowed or rebound local and a type or trait
   shadowed in the test's module scope still refuse with the check lifted. A
   test the finding never related (a same-named type in another crate) is not
   judged.
4. That test's oracle is not what keeps the gap: at least one of its
   assertions names the owner and every assertion that names the owner is
   `strong` under the shared oracle strength owner (`OracleStrength`). A weak or
   medium check over the owner (`std::assert!(w.read() > 0)`,
   `std::assert_ne!(w.read(), 0)`) is a gap with the macro trusted too, so it is
   never withheld. The assertion text, not the test body, is the subject, so an
   unrelated strong assertion cannot lend its strength to a weak one over the
   owner, and an unrelated weak one cannot hide a strong one. Strong does not
   mean credited: with the macro trusted the finding may still read a gap for
   another reason (for example `observation_unconfirmed`). The rule does not
   compute that verdict and so does not claim the trusted reading is `exposed`;
   it declines to say a gap exists only when the oracle is not the reason.
5. Target-root placement is unavailable (`TargetRoots.disabled`).

## Behavior

The finding becomes `static_unknown` with `static_limit_kind`
`rust_inherent_receiver_unestablished` and stop reason
`gap_evidence_unresolved`, through the shared `withhold_gap` owner. Its next
step names the blocked test. The classifier records the decision with a
marker while the run-shared syntax memo is in scope; the language post-pass
consumes the marker and removes it on every path.

Nothing is credited: the relation stays `weak_token_substring`, the test gains
no `direct_owner_call` and no grip, and the repository consumer
(`test_grip_evidence`, which shares `inherent_owner_call_resolves`) is
unchanged. Where that consumer reaches the same cause it already reads
`opaque` (an unknown, not a gap), so it needs no second rule.

## Non-Goals

- Crediting the call, trusting the macro, or resolving target placement.
- Withholding when the receiver would not resolve even with the macro trusted.
- Withholding when placement is available and the standard macro is rebound
  (`std` renamed to another package, `use fake as std`): that is a real
  source-level refusal and keeps its gap.
- Withholding a weak or medium oracle, or a mixed test where any assertion over
  the owner is not strong: the gap stands (a mixed test with a strong
  assertion also keeps today's gap, by the same doubt-keeps-the-gap rule).
- Lifting any macro-binding refusal that does not depend on placement. Scoped
  and crate-local names (`scoped_names_for`, `crate_names_for`), source
  provenance, the file's own manifest, source-level rebinding of `std`/the
  macro and module declarations stay in force. A visible `macro_rules!
  assert_eq` above the owner keeps its gap, and so does a macro of that name in
  any other module of the test's own package (`#[macro_use] mod noop;`).
- Bare-name ambiguity is scoped, not lifted. With placement unavailable, the
  workspace-wide ambiguity set (`trusted_macro_ambiguities`) is replaced by the
  names the test's own package (path-derived package root) makes ambiguous
  without a scope, scanned from only that package's files, plus every withheld
  file's. A site in another package (a `pub use core::*` glob or an exported
  `macro_rules!` in a sibling crate) reaches this test only if that crate's file
  is mounted into the test's target, which unplaced placement can neither
  establish nor rule out; it is part of the unplaced doubt, so a bare
  `assert_eq!` in that workspace is withheld, not a gap. A file whose package
  cannot be named from its path counts as the test's own, and a test whose
  package cannot be named keeps the workspace-wide answer.
- A new schema version or a new stop reason.

## Required Evidence

- `crates/ripr/tests/receiver_unestablished_limit.rs` drives the built binary
  over the linked-A / generated-B workspace (members `a`, `b`, `fakestd`):
  `a` is `[lib] test = false`, holds `Widget` and its `#[cfg(test)]` test with
  `std::assert_eq!`; `b` renames `std` to `fakestd` and mounts `a`'s file
  through a build-script `include!`. `--mode ready` is required, because the
  default mode does not index `b`.
  - The diff consumer reads `static_unknown` with the new limit, never a gap
    and never `exposed`; it read `reachable_unrevealed` with
    `weak_token_substring` before.
  - The repo consumer's `impl Widget::read` seam is not `strongly_gripped`, has
    no `direct_owner_call` related test and is not actionable.
- Negatives that keep their gap: a wrong receiver type, a message-only operand,
  a shadowed local, and the same workspace with a resolvable `#[path]` (placement
  available, `std` bound to `fakestd`).
- More negatives that keep their gap: `std::assert!(w.read() > 0)`,
  `std::assert_ne!(w.read(), 0)`, a weak assertion over the owner beside an
  unrelated strong one, a strong assertion on another type beside a weak one
  over the owner, a visible `macro_rules! assert_eq` above the owner, an
  `#[ignore]` test, a call outside the assertion (`let read = w.read();
  std::assert_eq!(read, 4)`, already `direct_owner_call` so nothing is blocked
  by the macro), and a same-named type with its own test in another crate. One
  positive keeps the withhold when an unrelated weak assertion sits beside the
  strong one.
- Mutation experiments, each run against the integration test (one at a time,
  reverted) and killed by the named test. Dropping the conjunct that the call
  is not already `direct_owner_call` with the macro check enforced:
  `a_call_outside_the_assertion_is_not_blocked_by_the_macro`. Dropping
  `!is_ignored`: `an_ignored_test_does_not_trigger_the_withhold`. `all` to `any`
  over the owner assertions, and dropping the strength requirement:
  `a_weak_oracle_keeps_its_gap_even_though_the_macro_is_unverifiable`. Dropping
  the mention filter: `an_unrelated_weak_assertion_does_not_lift_the_withhold`.
  Scoping off (the workspace-wide ambiguity set read as before):
  `another_crates_glob_reexport_is_part_of_the_unplaced_doubt`. The own-package
  scan seeing no file: `an_own_crate_macro_in_another_module_keeps_the_bare_assertion_gap`.
  Judging every indexed test instead of the finding's related tests:
  `a_same_named_type_in_another_crate_does_not_trigger_the_withhold`. Lifting
  the whole macro check instead of the placement input:
  `a_visible_macro_that_replaces_the_bare_assertion_keeps_its_gap`.
  Unexercised: the "at least one assertion names the owner" guard has no
  discriminating test (mutating it away leaves every test green) and this spec
  does not claim it is redundant or proved. It is kept only because it fails
  closed (no assertion naming the owner keeps the gap). The former `!test.assertions.is_empty()` and
  `test.body.contains(owner)` conjuncts were removed as subsumed by the
  assertion-text check. The removal of the post-pass itself and the
  receiver-negative mutations (dropping the comparison, dropping the placement
  requirement) were not re-run for this revision. Every mutation above was
  re-run on this revision's final source and killed as named; the two
  own-package mutations each also fail the same-file macro negative.
- Cache generation decision: `CACHE_SCHEMA_VERSION` (1.99) and the sharded and
  compact generations (0.106) are not bumped. Every cache in `seam_cache.rs`
  (`SeamFacts`, `CompactClassifiedSeams`, sharded) stores `ClassifiedSeam`
  values (seam, `TestGripEvidence`, `SeamGripClass`) produced by
  `test_grip_evidence`/`seam_classification`; neither names `Finding` or the
  `apply_probe_and_oracle_limits` post-pass where this rule runs, so no cached
  classification can bypass it. The shared `inherent_owner_call_resolves`
  still enforces the macro check, and the lifted reading (`PinGate::trusted`)
  is reachable only from this rule's judgment, so cached seam output is
  unchanged. No stale-generation replay is run because no generation moves.
- Boundary actually implemented (bind the text to this): withhold only for a
  `std`/`core`-rooted assertion path, when placement is unavailable, for a
  non-ignored related test whose owner-call resolves once only the placement
  input is lifted (bare-name ambiguity read from the test's own package only)
  and whose assertions naming the owner are all `strong`. The
  qualifying positive is `std::assert_eq!(Widget::new().read(), 4)` behind the
  generated include; the `> 0` and `assert_ne!(.., 0)` controls sit beside it.
- Instrumented cause for the serde rows
  `serde-derive-rename-field-upper` and `serde-derive-rename-variant-lower`:
  their tests assert through a bare `assert_eq!`; of the three bare-name
  refusals only the workspace-wide one fired, and its only `assert_eq` sites
  were `serde_core/src/lib.rs` lines 111 and 113 (`pub use core::*;` and
  `pub use std::*;`, `ForeignGlob`, not crate-local), a different package from
  the test. With the scoping above both rows read `static_unknown` /
  `rust_inherent_receiver_unestablished` again (`limited`).
- Executed receipt: with `crates/ripr/src` checked out from the base
  `a45614b88` (no rule) and this revision's integration test, the same-file
  no-op `macro_rules! assert_eq` case
  (`a_visible_macro_that_replaces_the_bare_assertion_keeps_its_gap`) reads a gap
  and passes; so do the weak-oracle, ignored-test, bare-macro, call-outside and
  other-crate negatives, while the two positives
  (`an_unplaced_standard_assertion_over_the_owner_call_is_a_named_limitation`,
  `an_unrelated_weak_assertion_does_not_lift_the_withhold`) fail, as they must
  without the rule. That the previous revision of the rule withheld the no-op
  macro case was a review finding and was not executed here.

## Acceptance Examples

- Linked-A / generated-B workspace, `Widget::read` mutated from `4` to `5`
  with `--mode ready`: the diff finding reads `static_unknown` with
  `rust_inherent_receiver_unestablished`; before this rule it read
  `reachable_unrevealed` with `weak_token_substring`.
- Same workspace with `Other::new().read()` asserted: the finding stays a gap.
- Same workspace with `b` mounting the file through a literal `#[path]`: the
  finding stays a gap.

## Static limit row

| `static_limit_kind` | Meaning |
| --- | --- |
| `rust_inherent_receiver_unestablished` | A related test calls the changed inherent method inside a standard assertion and the receiver resolves if that macro is the standard one, but the macro binding cannot be verified because target-root placement is unavailable. The gap is withheld. |

## Fail-closed statement

The rule only withholds. It runs after every other limit and never changes a
class toward `exposed`; a wrong actionable signal is worse than a missed
advisory, so a doubt about the trigger keeps the current gap and a doubt about
the macro keeps the current non-credit.

## Test Mapping

See `.ripr/traceability.toml`.

## Implementation Mapping

- `crates/ripr/src/analysis/classify/receiver_unestablished.rs` owns the rule.
- `crates/ripr/src/analysis/classify/related_tests.rs`:
  `inherent_owner_call_resolves_if_macros_trusted` and `PinGate` read the
  owner-call relation with only the placement input of the standard-root check
  lifted.
- `crates/ripr/src/analysis/classify/owner_pin.rs`:
  `macro_binding_is_unambiguous_if_unplaced` is the same decision as
  `targeted_macro_binding_is_unambiguous` with that one input lifted.
- `crates/ripr/src/analysis/classifier.rs` marks the finding;
  `crates/ripr/src/analysis/language/rust/mod.rs` consumes the marker.
- `crates/ripr/src/domain/language.rs` owns the wire value.

## Metrics

- `verdict_corpus_false_actionable_rate`

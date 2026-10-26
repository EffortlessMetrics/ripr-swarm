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
assertion macro binding is verified, and that needs target-root placement: which
Cargo target compiles the test file, and so which `assert_eq!` it sees. An
unresolved `include!` or `#[path]` anywhere in the workspace (for example
`include!(concat!(env!("OUT_DIR"), "/plain.rs"))`) may mount any file into
another crate, so placement is unavailable and the binding cannot be verified.

The refusal then dropped the relation to `weak_token_substring` and the finding
to a gap: a certain "no test notices this" claim, and an actionable repair
signal, resting on a receiver that was never shown to differ.

## Trigger

All of these hold, and any doubt keeps today's behavior:

1. The finding is a Rust gap (`weakly_exposed` or `reachable_unrevealed`) with
   no earlier `static_limit_kind`.
2. The owner is an inherent method with a receiver
   (`requires_inherent_receiver_identity`).
3. Some non-`#[ignore]` test that is one of the finding's own related tests
   has an owner-call relation that is not `direct_owner_call` with the
   standard-root check enforced, and **is** `direct_owner_call` with only the
   placement input of that check lifted. The comparison runs the complete
   owner-call relation, so a wrong receiver, a shadowed or rebound local and a
   type or trait shadowed in the test's module scope still refuse.
4. At least one assertion names the owner and every assertion that names the
   owner is `strong` under the shared `OracleStrength` owner. A weak or medium
   check over the owner is a gap with the macro trusted too, so it is never
   withheld. The assertion text, not the test body, is the subject. Strong does
   not mean credited, and the rule does not claim the trusted reading is
   `exposed`; it declines to say a gap exists only when the oracle is not the
   reason.
5. Target-root placement is unavailable (`TargetRoots.disabled`).

## Behavior

The finding becomes `static_unknown` with `static_limit_kind`
`rust_inherent_receiver_unestablished` and stop reason
`gap_evidence_unresolved`, through the shared `withhold_gap` owner. Its next
step names the blocked test. The classifier records the decision with a marker
while the run-shared syntax memo is in scope; the language post-pass consumes
the marker and removes it on every path.

Nothing is credited: the relation stays `weak_token_substring`, the test gains
no `direct_owner_call` and no grip, and the repository consumer
(`test_grip_evidence`, which shares `inherent_owner_call_resolves`) is
unchanged; where it reaches the same cause it already reads `opaque`.

## Non-Goals

- Crediting the call, trusting the macro, or resolving target placement.
- Withholding when the receiver would not resolve even with the macro trusted,
  when placement is available and the standard macro is rebound, or for a weak
  or medium oracle (the gap stands).
- Lifting any macro-binding refusal that does not depend on placement: scoped
  and crate-local names, source provenance, the file's own manifest,
  source-level rebinding of `std`/the macro and module declarations. A visible
  `macro_rules! assert_eq` above the owner keeps its gap.
- Bare-name ambiguity is scoped, not lifted. With placement unavailable, the
  workspace-wide ambiguity set is replaced by the names the test's own scope
  makes ambiguous, scanned from only that scope's files plus every withheld
  file's. The own scope is the test's package, every file a resolved `mod`
  declaration mounts from it (including a literal `#[path]` into another
  package), the declaring parents of the test file, and every package it
  reaches through a lexical import edge (`use dep::*`, `use dep::name`,
  `extern crate dep`) to a fixpoint. An import is matched to a package by
  directory name and fails closed to every package when the name is a
  workspace package or macro-scope crate and matches no or several directories, the matching directory holds a crate of another
  name, any manifest in scope renames a dependency, or a manifest cannot be
  read. A site in a package the scope never imports or mounts is part of the
  unplaced doubt, so a bare `assert_eq!` there is withheld, not a gap. The scan
  is lexical and over-approximate, which only keeps more gaps.
- A new schema version or a new stop reason.

## Required Evidence

- `crates/ripr/tests/receiver_unestablished_limit.rs` drives the built binary
  over the linked-A / generated-B workspace (members `a`, `b`, `fakestd`) with
  `--mode ready`: the diff consumer reads `static_unknown` with the new limit,
  never a gap and never `exposed` (it read `reachable_unrevealed` with
  `weak_token_substring` before), and the repo consumer's seam is not
  `strongly_gripped`, has no `direct_owner_call` related test and is not
  actionable.
- Negatives that keep their gap: a wrong receiver type, a resolvable `#[path]`
  with placement available, weak and medium oracles, a visible `macro_rules!
  assert_eq` above the owner, a literal `#[path]` mount of another package's
  macro, a glob import of a dependency macro, and a renamed dependency behind a
  decoy directory. One positive keeps the withhold when an unrelated weak
  assertion sits beside the strong one, and another crate's glob re-export
  stays part of the unplaced doubt.
- The serde rows `serde-derive-rename-field-upper` and
  `serde-derive-rename-variant-lower` read `limited`: `serde_derive` does not
  import `serde_core`, whose `pub use core::*` sites stay outside its own scope.
- Unexercised guards, not claimed proved: the `#[ignore]` exclusion, the
  own-related-tests filter, the enforced-relation conjunct, the rename and
  mismatch manifest arms each on its own, the test file's own manifest under
  trust, crate-local and scoped names under trust, nested `src` and `extern
  crate` edges; and (1) the "at least one assertion names
  the owner" guard (fails closed); (2) the reach-every-package rules for an
  unmatched, duplicated or unreadable workspace name and hyphen normalisation;
  (3) the declaring parents of the test file; (4) the withheld-file refusals;
  (5) the `target_roots.disabled` and `std`/`core` conjuncts on the
  placement-input condition (the caller already requires unavailable placement);
  (6) the `toml::Table` parse-failure arm in the manifest-name mismatch loop
  (an unparsable manifest returns the conservative answer).
- Mutation experiments and the executed base receipt are retained with the
  hardening follow-up.
- The post-pass stores nothing in a seam cache. The identity gate does change
  `ClassifiedSeam` relations; stale entries are safe because the cache key
  includes the commit- or source-digest-bound analyzer identity, not a
  hand-bumped schema version.
- Known open: when the owner call sits outside the assertion
  (`let r = Widget::new().read(); std::assert_eq!(r, 4);`) the relation is
  already direct and only the oracle row is refused by the same unverifiable
  macro, so the finding stays a gap. The oracle-side limit names free-function
  owners only. `a_call_outside_the_assertion_is_not_blocked_by_the_macro`
  (hardening follow-up) pins today's behavior.
- Non-claim: the withhold checks the relation and oracle strength, not that
  the trusted reading would leave no other gap (for example a missing boundary
  input). A gap that holds in both macro worlds can read `static_unknown`.

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

The rule only withholds. It runs after the implicit-call and indirect-owner withholds and never changes a
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

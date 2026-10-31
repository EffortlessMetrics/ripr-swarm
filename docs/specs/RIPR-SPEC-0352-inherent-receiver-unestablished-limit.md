# RIPR-SPEC-0352: An inherent receiver blocked only by an unverifiable assertion macro is a named limitation, not a gap

Status: proposed

Owner: product-swarm

Created: 2026-10-24

Linked plan: release-0.11 trustworthy-verdicts scoreboard

Linked issues: #9611 (inherent-receiver identity), #10122 (unplaced workspace receiver)

Support-tier impact:

- None.
- Narrows when Rust findings claim a gap; promotes no language, surface or gate.

Policy impact:

- Register this spec in `policy/doc-artifacts.toml` and
  `.ripr/traceability.toml`.
- Add one `static_limit_kind` value (`rust_inherent_receiver_unestablished`)
  to `policy/output_contracts.txt`, `docs/OUTPUT_SCHEMA.md` and
  `docs/STATIC_LIMITS.md`. The schema version is unchanged; the RIPR-SPEC-0240
  `gap_evidence_unresolved` stop reason is reused.


## Problem

An inherent method owner relates to a test as `direct_owner_call` only when the
receiver's type is established (#9611). A call inside a standard assertion,
`std::assert_eq!(Widget::new().read(), 4)`, counts only when the assertion
macro binding is verified, which needs target-root placement. An unresolved
`include!` or `#[path]` anywhere in the workspace may mount any file into
another crate, so placement is unavailable and the binding cannot be verified.

The refusal then dropped the relation to `weak_token_substring` and the finding
to a gap, an actionable repair signal on a receiver never shown to differ.

## Trigger

All of these hold, and any doubt keeps today's behavior:

1. The finding is a Rust gap (`weakly_exposed` or `reachable_unrevealed`) with
   no earlier `static_limit_kind`.
2. The owner is an inherent method with a receiver
   (`requires_inherent_receiver_identity`).
3. Some non-`#[ignore]` test among the finding's own related tests has an
   owner-call relation that is not `direct_owner_call` with the standard-root
   check enforced, and **is** `direct_owner_call` with only the placement input
   of that check lifted. A wrong receiver, a shadowed or rebound local and a
   type or trait shadowed in the test's module scope still refuse.
4. At least one assertion names the owner and every assertion that names the
   owner is `strong` under the shared `OracleStrength` owner. A weak or medium
   check over the owner is a gap with the macro trusted too, so it is never
   withheld. Strong does not mean credited; the rule does not claim the trusted
   reading is `exposed`.
5. Target-root placement is unavailable (`TargetRoots.disabled`).

## Behavior

The finding becomes `static_unknown` with `static_limit_kind`
`rust_inherent_receiver_unestablished` and stop reason
`gap_evidence_unresolved`, through the shared `withhold_gap` owner. The
classifier records the decision with a marker while the run-shared syntax memo
is in scope; the language post-pass consumes the marker and removes it on every
path. Nothing is credited: the relation stays `weak_token_substring`, and the
repository consumer (`test_grip_evidence`, which shares
`inherent_owner_call_resolves`) is unchanged and reads `opaque` for the same
cause.

## Non-Goals

- Crediting the call, trusting the macro, or resolving target placement.
- Withholding when the receiver would not resolve even with the macro trusted,
  when placement is available and the standard macro is rebound, or for a weak
  or medium oracle.
- Lifting any macro-binding refusal that does not depend on placement: scoped
  and crate-local names, source provenance, the file's own manifest,
  source-level rebinding of `std`/the macro and module declarations. A visible
  `macro_rules! assert_eq` above the owner keeps its gap.
- Bare-name ambiguity is scoped, not lifted. With placement unavailable, the
  workspace-wide ambiguity set is replaced by the names the test's own scope
  makes ambiguous: the test's package, every file a resolved `mod` declaration
  mounts from it (including a literal `#[path]` into another package), the
  declaring parents of the test file, and every package it reaches through a
  lexical import edge (`use dep::*`, `use dep::name`, `extern crate dep`) to a
  fixpoint. An import fails closed to every package when its name matches no or
  several directories, the directory holds a crate of another name, any
  manifest in scope renames a dependency, or a manifest cannot be read. The scan
  is lexical and over-approximate, which only keeps more gaps.

## Required Evidence

- `crates/ripr/tests/receiver_unestablished_limit.rs` drives the built binary
  over the linked-A / generated-B workspace with `--mode ready`: the diff
  consumer reads `static_unknown` with the new limit (it read
  `reachable_unrevealed` with `weak_token_substring` before), and the repo
  consumer's seam is not `strongly_gripped`, has no `direct_owner_call` related
  test and is not actionable.
- Negatives that keep their gap: a wrong receiver type, weak and medium oracles,
  a resolvable `#[path]` with placement available, a visible `macro_rules!
  assert_eq`, and the factory-declaration, shadow, qualifier and generic
  boundaries of the identity gate.
- The serde rows `serde-derive-rename-field-upper` and
  `serde-derive-rename-variant-lower` read `limited`. Their receiver is a
  bare-imported unit variant (`UpperCase.apply_to_field`). The identity gate
  resolves it only when exactly one non-aliased `use` of the enum's glob or of
  the variant brings it in, the enum has one declaration, the spelling is not a
  prelude value and no macro or same-named item can rebind it (#8782); then
  only the placement input is missing and this rule withholds the gap.
- Unexercised guards, not claimed proved here (the hardening follow-up, #10666
  and PR3, adds their controls and mutation experiments): the own-scope
  negatives for a `#[path]` mount, glob or explicit import of a dependency
  macro and a renamed dependency behind a decoy directory; the `#[ignore]`
  exclusion, the own-related-tests filter, each manifest rename/mismatch arm,
  crate-local and scoped names under trust, the reach-every-package rules and
  the `toml::Table` parse-failure arm.
- Known open: when the owner call sits outside the assertion the relation is
  already direct and only the oracle row is refused, so the finding stays a gap.
  The withhold checks relation and oracle strength, not that the trusted
  reading would leave no other gap, so a gap that holds in both macro worlds
  can read `static_unknown`.

## Acceptance Examples

- `Widget::read` mutated `4` to `5` with `--mode ready`: `static_unknown` with
  `rust_inherent_receiver_unestablished`; with `Other::new().read()`, a gap.

## Static limit row

| `static_limit_kind` | Meaning |
| --- | --- |
| `rust_inherent_receiver_unestablished` | A related test calls the changed inherent method inside a standard assertion and the receiver resolves if that macro is the standard one, but the macro binding cannot be verified because target-root placement is unavailable. The gap is withheld. |

## Test Mapping

See `.ripr/traceability.toml`.

## Fail-closed statement

The rule only withholds, after the implicit-call and indirect-owner withholds,
and never moves a class toward `exposed`; a doubt keeps the current gap.

## Implementation Mapping

- `classify/receiver_unestablished.rs` owns the rule; `classify/related_tests.rs`
  (`inherent_owner_call_resolves_if_macros_trusted`, `PinGate`) and
  `classify/owner_pin.rs` (`macro_binding_is_unambiguous_if_unplaced`) read the
  existing decisions with only the placement input lifted.
- `analysis/classifier.rs` marks the finding; `analysis/language/rust/mod.rs`
  consumes the marker; `domain/language.rs` owns the wire value.

## Metrics

- `verdict_corpus_false_actionable_rate`

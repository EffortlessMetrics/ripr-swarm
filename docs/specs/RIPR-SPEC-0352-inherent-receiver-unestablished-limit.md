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
3. Some indexed test that is not `#[ignore]`, has an assertion and mentions the
   owner's name has an owner-call relation that is not `direct_owner_call`
   with the assertion macro gate on, and **is** `direct_owner_call` with the
   gate off. The comparison runs the complete owner-call relation, so a wrong
   or message-only receiver, a shadowed or rebound local and a type or trait
   shadowed in the test's module scope still refuse with the gate off.
4. Target-root placement is unavailable (`TargetRoots.disabled`).

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
- Each of the positive and the negatives is discriminating: removing the
  post-pass fails the positive; dropping the gate-off comparison or the
  placement requirement fails the negatives.
- `cargo xtask verdict-corpus check-all`: only `serde-derive-rename-field-upper`
  and `serde-derive-rename-variant-lower` move, from `credited` to `limited`.
  `serde_core` mounts generated code with `include!(concat!(env!("OUT_DIR"),
  ..))`, so placement is unavailable and a credit cannot be established
  soundly. They are not known-false rows.

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
  `inherent_owner_call_resolves_if_macros_trusted` and `PinGate` factor the
  macro gate out of the owner-call relation.
- `crates/ripr/src/analysis/classifier.rs` marks the finding;
  `crates/ripr/src/analysis/language/rust/mod.rs` consumes the marker.
- `crates/ripr/src/domain/language.rs` owns the wire value.

## Metrics

- `verdict_corpus_false_actionable_rate`

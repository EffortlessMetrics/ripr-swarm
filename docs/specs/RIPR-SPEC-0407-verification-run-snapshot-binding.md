# RIPR-SPEC-0407: Verification run snapshot binding and requested/prepared compatibility

Status: proposed

Owner: analysis / verification-plan

Created: 2026-11-03

Linked issues:

- [#6347](https://github.com/EffortlessMetrics/ripr-swarm/issues/6347) (this slice: the passive VP25A binding model)
- PR [#12351](https://github.com/EffortlessMetrics/ripr-swarm/pull/12351)

Support-tier impact:

- None. The binding is an internal `pub(crate)` decision surface with no CLI,
  JSON, SARIF, LSP or MCP output and no support-tier effect.

Policy impact:

- Register in `policy/doc-artifacts.d/RIPR-SPEC-0407.toml`.
- Add `docs/spec-index.d/RIPR-SPEC-0407.md`.
- Register `.ripr/traceability.toml` behavior `RIPR-SPEC-0407`.

## Problem

A verification plan names a requested source subject (committed head or range,
staged index, saved worktree, overlay, captured external). Before any provider
may run against a prepared snapshot, something must decide whether the prepared
subject is the one that was requested, without acquiring, materializing or
executing anything. A permissive comparison would let a run silently consume a
different capture policy, a changed captured-external path or a partial
inventory and still report a complete binding.

## Behavior

- `requested_prepared_compatibility` compares a requested subject with a
  prepared subject reference and returns `Compatible` or an `Incompatible`
  result naming the first load-bearing mismatch axis, checked in a fixed order:
  subject kind, envelope digest and authority, repository, cross-kind laws
  (committed versus dirty worktree, staged versus unstaged replacement, saved
  worktree changed after capture, overlay generation), base/head/tree, index,
  worktree, overlay, captured-external identity, staged-deletion set, tagged
  `extra` identity fields, capture policy, selected closure, decision-bearing
  inputs, resource/configuration, then closure and inventory completeness.
- Every tagged identity field participates: a differing `extra` map (for
  example a captured-external `original_path`) and a differing capture policy
  are incompatible, never silently compatible.
- The shared snapshot envelope is referenced by authority and digest; no rival
  envelope DTO is copied.
- `decide_snapshot_binding` records the compatibility, live-worktree relation
  and binding completeness, and computes a portable binding identity over the
  plan reference, the portable requested and prepared projections (including
  unsupported-inventory contents), compatibility, live relation and
  completeness. The concrete checkout-root spelling is excluded; limitations
  are sorted and deduplicated.
- `ready_for_execution_authorization` is always false in this slice.

## Required Evidence

- The committed hostile corpus `fixtures/verification_run_snapshot_binding/`
  (ten cases), including one pinned portable binding digest.
- Unit tests for policy mismatch, captured-external path mismatch,
  unsupported-inventory identity, plan-digest-only identity change, root
  relocation and reorder invariance.
- `cargo xtask check-fixture-contracts` validating per-case corpus structure.

## Non-Goals

No snapshot acquisition, materialization, provider execution, CI policy change,
cargo-proof integration, resume journey, or proof that providers read only the
prepared closure. No runtime mutation outcome or sufficiency claim.

## Acceptance Examples

- An exact committed pair is compatible and complete.
- A committed request against a prepared dirty worktree is rejected on
  `committed_vs_dirty_worktree`.
- A prepared subject with a different capture policy is rejected.
- Two captured-external subjects with different `original_path` values are
  rejected on `captured_external_identity`.
- Two bindings that differ only in unsupported-inventory entries have distinct
  binding identities; two prepared roots that differ only in checkout path have
  the same identity.

## Test Mapping

- `crates/ripr/src/analysis/verification_plan/snapshot_binding/tests.rs` —
  corpus acceptance, root relocation, plan identity, envelope reference.
- `crates/ripr/src/analysis/verification_plan/snapshot_binding/decide.rs` —
  `decide_tests` compatibility and identity discriminators.
- `xtask/src/fixture_contracts/general_validators.rs` — corpus structure gate.

## Implementation Mapping

- `crates/ripr/src/analysis/verification_plan/snapshot_binding/model.rs` —
  DTOs and portable projections.
- `crates/ripr/src/analysis/verification_plan/snapshot_binding/decide.rs` —
  compatibility law and binding identity.
- `fixtures/verification_run_snapshot_binding/` — hostile corpus.

## CI Proof

```bash
cargo test -p ripr --lib analysis::verification_plan::snapshot_binding
cargo xtask check-fixture-contracts
cargo xtask check-traceability
cargo xtask check-doc-artifacts
cargo xtask precommit
```

## Metrics

- `unit_test_pass_rate`

## Failure Modes

- A prepared subject that differs on any compared axis is incompatible and
  names the first axis; it is never reported complete.
- An incomplete closure, partial or unsupported prepared completeness, or any
  unsupported inventory entry prevents a complete binding.
- A corpus case that loses its expected axis or requested subject fails the
  fixture-contract gate.

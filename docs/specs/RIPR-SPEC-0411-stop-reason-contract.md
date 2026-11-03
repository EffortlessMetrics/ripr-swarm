# RIPR-SPEC-0411: Stop reason contract

Status: proposed

Owner: product-analysis

Created: 2026-11-02

Linked issues:

- [#5514](https://github.com/EffortlessMetrics/ripr-swarm/issues/5514) (spec coverage survey under #5512)

Linked specs:

- [RIPR-SPEC-0319](RIPR-SPEC-0319-classification-keyed-guidance.md) — imperative next steps
- [RIPR-SPEC-0114](RIPR-SPEC-0114-rust-transitive-reach-limitation.md),
  [RIPR-SPEC-0117](RIPR-SPEC-0117-rust-macro-reach-limitation.md),
  [RIPR-SPEC-0118](RIPR-SPEC-0118-rust-integration-public-api-path-limitation.md),
  [RIPR-SPEC-0119](RIPR-SPEC-0119-rust-direct-test-macro-call-limitation.md) — reach stops
- [RIPR-SPEC-0159](RIPR-SPEC-0159-bounded-helper-transfer.md) — helper identity
- [RIPR-SPEC-0168](RIPR-SPEC-0168-unsafe-boundary-probes.md) — unsafe obligation
- [RIPR-SPEC-0240](RIPR-SPEC-0240-unknown-not-a-gap.md) — gap evidence
- [RIPR-SPEC-0028](RIPR-SPEC-0028-python-preview-static-facts.md) — Python static limits

Support-tier impact: none (documentation and enforcement of existing behavior)

Policy impact: register in `policy/doc-artifacts.toml` and `.ripr/traceability.toml`

## Problem

`StopReason` is the machine-readable "why" on unknown and limited findings.
Several values were documented only in `docs/OUTPUT_SCHEMA.md` or implied by
substring heuristics in `analysis/classify/decision.rs`. Unspecced triggers can
drift silently, and wire tokens documented without a producer promise refusals
ripr never makes.

## Behavior

Each `stop_reasons` entry on a finding must come from one of the roles below.
Heuristic annotations and unknown-class backfills **do not** change
`ExposureClass` by themselves; reach, gap-admission, helper-identity, and
Python static-limit mappings **may** withhold or move class as their owning
specs already state.

| Wire value | Role | Changes class? | Trigger (current main) | Owning spec |
| --- | --- | --- | --- | --- |
| `max_depth_reached` | reserved wire | no | not emitted | RIPR-SPEC-0411 |
| `external_crate_boundary` | reserved wire | no | not emitted | RIPR-SPEC-0411 |
| `feature_unknown` | reserved wire | no | not emitted | RIPR-SPEC-0411 |
| `infection_evidence_unknown` | unknown-class backfill | no | `ensure_unknown_stop_reason` when class is `infection_unknown` and the list would be empty | RIPR-SPEC-0411 |
| `propagation_evidence_unknown` | unknown-class backfill | no | same for `propagation_unknown` | RIPR-SPEC-0411 |
| `static_probe_unknown` | unknown-class backfill | no | same for `static_unknown` | RIPR-SPEC-0411 |
| `fixture_opaque` | heuristic annotation | no | related test body contains `fixture`, `builder`, or `arb_` | RIPR-SPEC-0411 |
| `async_boundary_opaque` | heuristic annotation | no | probe expression mentions `async`, `spawn`, or `await` | RIPR-SPEC-0411 |
| `proc_macro_opaque` | heuristic annotation | no | probe expression contains a `name!(` invocation (not `!=`) | RIPR-SPEC-0411 |
| `no_changed_rust_line` | heuristic annotation | no | changed line has no owning Rust function and is not a compile-time assertion | RIPR-SPEC-0411 |
| `compile_time_invariant` | compile-time shape | no | changed line is a compile-time assertion (`const_assert!`, etc.) without an owner function | RIPR-SPEC-0411 |
| `unsafe_obligation` | unsafe obligation | no | `static_unknown` probe labels `unsafe fn` or unchecked-assumption block | RIPR-SPEC-0168 |
| `transitive_reach_unresolved` | reach limitation | yes | bounded reach walk stops at an unresolved internal edge | RIPR-SPEC-0114 |
| `macro_reach_unresolved` | reach limitation | yes | reach candidate stops at an unexpanded same-repo macro | RIPR-SPEC-0117 |
| `gap_evidence_unresolved` | gap admission | yes | gap withheld because framework/oracle evidence is unreadable | RIPR-SPEC-0240 |
| `helper_identity_unresolved` | helper identity | yes | ambiguous same-named helper on the test entry path | RIPR-SPEC-0159 |
| `dynamic_dispatch_unresolved` | static-limit mapping | yes | Python preview maps `StaticLimitKind::DynamicDispatch` | RIPR-SPEC-0028 |

The Trigger column names the primary producer on current main, not the only
one. Other adapters build the same wire values through their own paths: the
TypeScript classifier assigns the unknown-class backfill, transitive-reach and
gap-evidence values directly, and Rust Verus-region handling
(`language/rust/probes.rs`) attaches `proc_macro_opaque`. A producer may emit a
wire value only for the role in this table; a new trigger for an existing value
updates its row.

Heuristic annotations run in `decision::stop_reasons` before classification
backfill; they only add parallel reasons. Unknown-class backfill runs after
classification via `ensure_unknown_stop_reason`.

### Reserved wire values

`max_depth_reached`, `external_crate_boundary`, and `feature_unknown` remain
valid JSON tokens with human glosses and next-step text (RIPR-SPEC-0319), but
**no production classifier path on current main pushes them** onto a finding.
Emitting them requires a future spec slice that names the trigger and adds a
discriminating test. Until then, OUTPUT_SCHEMA lists them with this reservation
instead of implying live refusals.

Retiring a wire value requires a schema-version bump and migration note; this
slice does not remove tokens.

## Required Evidence

- `analysis/classify/stop_reason_contract.rs` holds the exhaustive registry and
  tests that no file under `src/analysis` builds or pushes a reserved wire
  (match-arm reads are not emissions), with a unit test that the detector flags
  pushes, split pushes and `vec!` literals.
- The spec table above has a row for every wire value (enforced by test).
- `docs/OUTPUT_SCHEMA.md` carries the reserved-wire note under `stop_reason`
  values.

## Non-Goals

- Implementing depth, external-crate, or cfg-feature emitters.
- Changing exposure classes, JSON schema version, or next-step wording beyond
  what existing specs already own.
- Replacing per-reach specs (0114–0119) or gap-admission specs (0240 family).

## Acceptance Examples

1. A finding on a changed line outside any Rust owner function reads
   `no_changed_rust_line` (and not `static_probe_unknown` alone) when heuristics
   apply; class stays whatever the stage pipeline already chose.
2. A Python preview finding limited by dynamic dispatch maps
   `dynamic_dispatch_unresolved` per RIPR-SPEC-0028; no Rust finding emits
   `max_depth_reached` on current main.
3. OUTPUT_SCHEMA lists `max_depth_reached`, `external_crate_boundary`, and
   `feature_unknown` with the RIPR-SPEC-0411 reserved-wire note.

## Test Mapping

- `crates/ripr/src/analysis/classify/stop_reason_contract.rs::tests::stop_reason_contract_registry_is_exhaustive_and_unique`
- `crates/ripr/src/analysis/classify/stop_reason_contract.rs::tests::reserved_wire_stop_reasons_have_no_production_push_sites`
- `crates/ripr/src/analysis/classify/stop_reason_contract.rs::tests::stop_reason_contract_spec_documents_every_wire_value`

## Implementation Mapping

- `crates/ripr/src/domain/probe.rs` — wire enum and glosses
- `crates/ripr/src/analysis/classify/decision.rs` — heuristics and backfill
- `crates/ripr/src/analysis/classify/stop_reason_contract.rs` — registry (#5514)
- Reach and gap producers named in the table above

## Metrics

Count of documented stop-reason wire values with a registered emission role
over total `StopReason` variants (17 of 17); count of reserved-wire tokens
with zero production push sites (3 of 3).

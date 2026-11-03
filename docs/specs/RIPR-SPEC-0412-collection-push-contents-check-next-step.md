# RIPR-SPEC-0412: Collection push next step names a contents check only on the repair route's precondition

Status: proposed

Owner: analysis / classify

Created: 2026-11-03

Linked issues:

- [#12456](https://github.com/EffortlessMetrics/ripr-swarm/issues/12456)
- [#11402](https://github.com/EffortlessMetrics/ripr-swarm/issues/11402) (the repair route this wording follows)
- PR [#12549](https://github.com/EffortlessMetrics/ripr-swarm/pull/12549)

Support-tier impact:

- None. Guidance wording only; no repair signal, class, or schema changes, and
  the canonical boundary remains [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register in `policy/doc-artifacts.d/RIPR-SPEC-0412.toml`.
- Add `docs/spec-index.d/RIPR-SPEC-0412.md`.
- Register `.ripr/traceability.d/collection-push-contents-next-step.toml`.

## Problem

For a free function that pushes into a `&mut Vec`/`&mut String` parameter and a
test that reads the passed local back only by a count that pins the single push,
the repair route (#11402) asks for an exact contents check. The diff-side
recommended next step and the LSP hover still said "mock expectation, event
observer, or persistence assertion", which contradicts that route. A wrong
repair signal is worse than a missed one, so the new wording must not reach
pushes the repair route would not propose a contents check for.

## Behavior

- The next step is `Add an exact check of the full contents the test reads back
  after the call.` only when the repair route's own condition holds: a
  side-effect probe or seam whose expression is a direct collection mutation
  and whose followed push reaches a local the test reads back by a count that
  pins the single push (`test_count_pins_pushed_collection` diff-side; the
  propagation stage's count-pinned marker for seams). That rule applies the
  sole-unconditional-push-into-a-`&mut Vec`/`String`-parameter owner shape
  (`sole_pushed_buffer_at`).
- Stage states plus a `x.push(..)` receiver are not sufficient. A wrong,
  aliased, conditional, helper-routed, or otherwise unfollowed receiver keeps
  the existing guidance.
- No class, exposure, repair-packet eligibility, or JSON schema changes.

## Required Evidence

- Unit tests: next-step wording, gate equals the repair-route decision, hover
  positive and negative.
- Positive golden `collection_push_contents_check_next_step`; the existing
  conditional (`rust_direct_collection_count_payload`), wrong/aliased
  (`collection_argument_*`) and `out_parameter_*` goldens stay byte-identical.
- Every checked-in golden whose next step changes is blessed from a real run
  and reviewed as a correct repair signal.

## Non-Goals

No new propagation rule, no repair-route change, no mutation-outcome claim.

## Acceptance Examples

- `fn log_line(buf: &mut Vec<String>, m: &str) { buf.push(m.to_string()); }`
  with a test asserting `buf.len() == 1` after the call: next step asks for an
  exact contents check.
- The same push where the test passes a different local, or the push is
  conditional: the mock-expectation guidance is unchanged.

## Test Mapping

- `crates/ripr/src/analysis/classify/decision.rs` — `recommended_next_step` tests.
- `crates/ripr/src/lsp/hover.rs` — `seam_hover_tests`.
- `crates/ripr/src/analysis/repair_route.rs` — gate matches the repair-route decision.
- `fixtures/collection_push_contents_check_next_step/` — end-to-end golden.

## Implementation Mapping

- `crates/ripr/src/analysis/repair_route.rs` — gate and wording constant.
- `crates/ripr/src/analysis/classifier/finding.rs` — diff-side wiring.
- `crates/ripr/src/analysis/classify/decision.rs` — next-step selection.
- `crates/ripr/src/lsp/hover.rs` — hover wording.

## CI Proof

```bash
cargo test -p ripr --lib collection_contents
cargo xtask goldens check
cargo xtask check-traceability
cargo xtask check-doc-artifacts
cargo xtask precommit
```

## Metrics

- `unit_test_pass_rate`

## Failure Modes

- A push the repair route would not follow gets contents-check wording.
- A hover or diff next step contradicts the repair-route refusal for the same
  seam.

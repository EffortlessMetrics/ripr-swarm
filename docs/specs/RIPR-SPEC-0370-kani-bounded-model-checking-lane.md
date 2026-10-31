# RIPR-SPEC-0370: Kani bounded model checking lane states exactly what a proof covers and never passes by absence

Status: proposed

Owner: repository / policy

Created: 2026-10-31

Linked proposal:

- None yet

Linked ADRs:

- None yet

Linked plan:

- None yet

Linked issues:

- None yet

Linked PRs:

- None yet

Support-tier impact:

- None. Kani proofs are an internal engineering check on `ripr`'s own code.
  They add no user-visible claim and change no finding vocabulary or tier
  (see [support tiers](../status/SUPPORT_TIERS.md)).

Policy impact:

- Register this spec in `policy/doc-artifacts.toml` (via
  `policy/doc-artifacts.d/`) and `.ripr/traceability.d/`.
- Adds the `kani` lane to `policy/ci-lane-whitelist.toml`, the
  `.github/workflows/kani.yml` workflow and the `cargo xtask kani` command.
- Adds `unexpected_cfgs` check-cfg for `cfg(kani)` to the workspace lints.

## Problem

`ripr` has no formal verification. Some load-bearing logic is a small pure
core (for example the actionability flip, which must stay fail-closed) where a
bounded model check can establish a property for every input in a stated
domain, which example-based tests cannot. Without a lane that is honest about
what ran, a missing tool, a harness that never executed or a `cover!` that is
unreachable would read as a pass.

## Behavior

### Harnesses

- A harness is a `#[kani::proof]` function in a `#[cfg(kani)]` module placed
  next to the code it proves. Ordinary builds never compile it.
- A harness calls the production function. It does not prove a copy.
- Input bounds are explicit `kani::assume` statements with a comment naming
  the domain they model.
- Each harness carries at least one `kani::cover!` on its interesting path.
  An unreachable cover makes the proof vacuous and is a failure.
- Code Kani cannot model (heavy string or `HashMap` work, async, process or
  filesystem effects) is not forced through a harness. A smaller pure core is
  proved, or the limit is recorded; a pass is never fabricated.

### The command

`cargo xtask kani [--allow-empty] [--harness <name>] [--default-unwind <n>]
[--timeout-secs <n>]` discovers the harnesses under `crates/ripr/src`, runs
`cargo kani -p ripr` once for them (Kani compiles the whole library, so a
run per harness would multiply the build) and writes `target/ripr/reports/kani.md` with one
row per harness. Each row is exactly one of:

- `VERIFIED`: Kani reported a successful verification and every `cover!` was
  satisfied. The claim is bounded by the harness's assumptions.
- `FAILED`: Kani reported a counterexample, or a `cover!` was unreachable.
- `NOT-ESTABLISHED`: the tool is missing, the run timed out, the harness was
  not found, or the output held no successful verdict.

The command exits nonzero when any harness is `FAILED` or `NOT-ESTABLISHED`,
when `cargo kani` is not runnable, and when no harness is found unless
`--allow-empty` is passed. `--allow-empty` is a local opt-out for sparse
checkouts; the CI lane never passes it.

### The lane

`.github/workflows/kani.yml` is advisory and not a required check. It runs
nightly, on dispatch and when `merge-ready` is applied to a pull request that
touches ripr source, with `kani-verifier` pinned to an exact version. See
[CI](../CI.md#advisory-kani-lane).

## What a proof does and does not claim

A `VERIFIED` row means: within the stated `kani::assume` bounds and unwinding
depth, Kani's model checker found no assertion violation, overflow, or panic
reachable from the harness, and the covered paths were reachable.

It does not claim:

- anything about inputs outside the harness's assumptions or beyond the unwind
  bound;
- anything about code the harness does not call, or about the full program;
- runtime mutation outcomes, test sufficiency or static exposure results. Real
  mutation testing remains a later independent authority;
- that the harness states the right property. A wrong assertion proves the
  wrong thing; harnesses are reviewed like any test oracle.

## Required Evidence

- The xtask unit tests pin the status classification: an explicit success
  verdict is required for `VERIFIED`, an unreachable cover fails, absence of a
  verdict is `NOT-ESTABLISHED`, and the lane verdict never passes on a missing
  tool or zero harnesses.
- The smoke harness `evidence_state_actionable_only_for_actionable` proves the
  actionability predicate holds for exactly one `EvidenceState` variant, with
  `cover!` on both outcomes.
- Non-vacuity: injecting a bug into `EvidenceState::is_actionable` makes the
  smoke harness fail (recorded in the introducing PR).

## Known Limits

- Kani compiles the whole `ripr` library; a run takes minutes and several GB of
  memory.
- Harness discovery is textual (`#[kani::proof` followed by `fn name`) and
  assumes a harness name is unique within `crates/ripr/src`.
- The lane trusts Kani's own output verdict lines; a change to their wording
  degrades to `NOT-ESTABLISHED`, never to a pass.

## Non-Goals

- A full mutation engine, proof system or coverage claim.
- Verifying string- or filesystem-heavy analysis code.
- Making the lane a required check.

## Acceptance Examples

1. With the tool absent, `cargo xtask kani` reports `NOT-ESTABLISHED` and fails.
2. With a harness whose `cover!` is unreachable, the row is `FAILED`.
3. With a clean harness set, every row is `VERIFIED` and the command succeeds.

## Test Mapping

- `xtask/src/kani_lane.rs`:
  `discovery_finds_named_harnesses_past_attributes_and_docs`,
  `classification_requires_an_explicit_success_verdict`,
  `unreachable_cover_makes_the_proof_vacuous`,
  `blocks_join_to_harnesses_and_a_missing_block_is_not_established`,
  `lane_never_passes_by_absence`,
  `arguments_parse_and_reject_unknown_flags`,
  `report_names_every_status_word`.

## Implementation Mapping

- `xtask/src/kani_lane.rs`: discovery, classification, report and verdict.
- `crates/ripr/src/kani_smoke.rs`: the smoke harness.
- `.github/workflows/kani.yml` and `policy/ci-lane-whitelist.toml`: the lane.
- `Cargo.toml`: `unexpected_cfgs` check-cfg for `cfg(kani)`.

## Metrics

- `unit_test_pass_rate`: the xtask tests above are required Rust tests.

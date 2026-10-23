# RIPR-SPEC-0311: A complete TypeScript packet offers a gap-bound repair start

Status: proposed

Owner: product / analysis

Created: 2026-10-22

Linked proposal:

- None yet

Linked ADRs:

- [ADR 0020](../adr/0020-repair-artifacts-carry-producer-identity.md)

Linked plan:

- None yet

Linked issues:

- #9103

Linked PRs:

- None yet

Support-tier impact:

- No tier change. TypeScript and Python stay `preview_advisory_only`.
- Claim boundaries remain governed by
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `.ripr/traceability.toml`.
- Additive `agent repair --gap-id/--base` flags and one additive, optional
  repair-attempt manifest member (`gap_binding`).

## Problem

A TypeScript finding with a complete repair packet had no start the repair
transaction accepted: `ripr check` printed "repair packet is not ready" and
`agent repair` required a Rust seam id. The verifier half (RIPR-SPEC-0310)
existed with no way to begin the attempt.

## Behavior

1. Gap-Bound Repair Start: for a TypeScript finding that is not static-limited,
   whose gap record passes the shared `validate_agent_gap_record_packet`, and
   when a base is known, `ripr check` prints
   `ripr agent repair --gap-id G --base B --phase before`.
2. `agent repair --gap-id G --base B --phase before` re-runs the diff-scoped
   check against `B`, looks the gap up, and runs the same shared validator.
   An unknown gap or a gap whose packet is not ready is refused ("the repair
   packet is not ready" or "names no current TypeScript gap") with no attempt
   and no workflow artifacts. The printed start line replays only `--base`
   and `--root`; a static-limited finding is refused here too, as `ripr check`
   would not offer it.
3. Flags are exclusive: `--gap-id` excludes `--seam-id`, requires `--base` and
   `--phase before`, accepts only a canonical TypeScript preview gap id,
   refuses Python gaps with their existing terminal reason, and refuses the
   Python repair-trust flags.
4. Acceptance writes `before.check-findings.json` and `gap-repair-packet.json`
   (packet `seam_id` is the gap id) and a manifest with `seam_id` = gap id and
   `gap_binding.base`. The edit cage is derived from the packet's test
   file but is not enforced in this slice: only the after phase consumes it
   and a gap-bound attempt has no after phase yet, so enforcement arrives with
   the gap-bound after slice. The persisted `next_command` is the manual
   by-gap verification route, never a `--phase after` command. A start
   refused after the workflow artifacts were written (including a dirty
   tracked non-test file) removes them and leaves no attempt manifest.
5. Status and recovery text for a gap attempt name the gap start and never
   `--seam-id`.
6. `agent repair --phase after` for a gap-bound attempt is refused naming the
   manual route (`agent verify --gap-id`); finishing and receipts are later
   slices.
7. Let-bound, imported and reach-barrier variants remain unrouted. Seam
   attempt manifests omit `gap_binding`, so their bytes and before commitment
   are unchanged.

## Required Evidence

- Parser tests: exclusivity, bad prefix, Python rejected.
- Edit-cage tests with a TypeScript gap packet (test file accepted,
  production and non-test files refused).
- Manifest round-trip with and without `gap_binding`.
- Status and recovery text for a gap attempt.
- Installed journey: the start line is printed, the before phase is accepted,
  the manifest and snapshot are recorded, the eight variants stay unrouted
  with negative start assertions, and Python assertions are unchanged.

## Non-Goals

- `--phase after`, receipts and verification run (later slices).
- Runtime mutation outcomes or sufficiency claims; results are static exposure
  evidence only.

## Acceptance Examples

1. On the installed TypeScript head fixture, `ripr check` prints
   `ripr agent repair --gap-id <gap> --base <base> --phase before`; running
   it is accepted, awaits the focused test edit, and records a manifest whose
   `seam_id` is the gap and whose `gap_binding.base` is the base.
2. The same start for a let-bound, imported or reach-barrier variant is
   refused with `the repair packet is not ready` and leaves no attempt.
3. `agent status` for the gap attempt never names `--seam-id`.

## Test Mapping

- `crates/ripr/tests/installed_python_typescript_journeys.rs`:
  `typescript_literal_threshold_packet_is_complete_and_controls_fail_closed`.
- `crates/ripr/src/cli/agent.rs`, `crates/ripr/src/app/repair_attempt/mod.rs`,
  `crates/ripr/src/app/agent_status.rs` and
  `crates/ripr/src/output/agent_seam_packets.rs` unit tests named in the
  traceability entry.

## Implementation Mapping

- `crates/ripr/src/app/gap_bound_repair.rs`: lookup, start spelling, manual
  verify route.
- `crates/ripr/src/cli/commands/agent.rs`: `run_gap_bound_before_phase`.
- `crates/ripr/src/app/repair_attempt/mod.rs`: `RepairAttemptGapBinding`,
  `begin_gap_bound_repair_attempt_with_identity`.
- `crates/ripr/src/app/task_first.rs`, `app/navigation.rs`,
  `output/human/triage.rs`: the printed start line.

## Metrics

- `unit_test_pass_rate`: the tests above are required Rust tests.

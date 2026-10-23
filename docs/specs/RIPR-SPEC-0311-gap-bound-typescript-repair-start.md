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
   file and is enforced by the gap-bound after phase (item 6). The
   persisted `next_command` is the ordinary `--attempt <id> --phase after`
   command. `gap_binding` records the `--base` spelling and `base_commit`, the
   full commit it resolved to; the before snapshot is produced against that
   commit. A start
   refused (including a dirty tracked non-test file, or a publication failure
   after the workflow artifacts were written) leaves no attempt manifest and
   restores the shared workflow artifacts to the bytes they had before the
   invocation: prior files survive byte-identical and only files the
   invocation created are removed.
5. Status and recovery text for a gap attempt name the gap start and never
   `--seam-id`.
6. `agent repair --phase after` for a gap-bound attempt writes
   `after.check-findings.json` through the same producer as the before
   snapshot, analyzed against `base_commit` (a relative `--base` such as
   `HEAD~1` names a different commit once the test is committed), and calls the
   shared verifier with the gap id. Closure is positive evidence only: the gap
   must be open before and `exposed` after; a gap that is absent from the after
   artifact, was never open, or still reads open is a typed refusal (exit 3,
   stdout `repair_after_refusal`) recorded against the attempt's after-phase
   retry budget, and the attempt stays `awaiting_edit`. A spent budget stops
   the phase with a restart that names `--gap-id`/`--base`. Only a closed gap
   reaches the edit cage, which is enforced: a violation or an incomparable
   cage ends the attempt terminally with the `repair_after_failure` envelope
   and no closure is reported. A compliant finish prints the
   `repair_after_result` envelope whose `verify.bound_gap` carries the closure.
   No receipt is issued for a gap attempt in this slice, and `agent status`
   says so instead of offering seam-bound `--seam-id` commands. The shared
   `validate_agent_gap_record_packet` stays the only actionability authority.
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
- Installed after-phase journeys: positive closure, not-closed refusal with
  retry, absent gap is not closure, cage violation, retry budget.

## Non-Goals

- Receipts and verification run for gap attempts (later slices).
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
4. After a committed test that pins the equality boundary, the persisted
   `--phase after` command exits 0, prints `verify.bound_gap` with
   `closed: true`, and leaves the attempt `ready_to_finish`.
5. A committed test that leaves the boundary unpinned exits 3 with
   `agent verify gap binding refused` and the attempt stays `awaiting_edit`;
   reverting the production change (the gap vanishes) exits 3 with `absence is
   not closure evidence`; a closing test committed beside `src/extra.ts` ends
   the attempt terminally; the sixth refusal stops on the retry budget.

## Test Mapping

- `crates/ripr/tests/installed_python_typescript_journeys.rs`:
  `typescript_literal_threshold_packet_is_complete_and_controls_fail_closed`,
  `gap_bound_after_phase_closes_the_gap_on_positive_evidence`,
  `gap_bound_after_phase_refuses_a_gap_that_is_not_closed_and_stays_retryable`,
  `gap_bound_after_phase_does_not_treat_an_absent_gap_as_closed`,
  `gap_bound_after_phase_enforces_the_edit_cage`,
  `gap_bound_after_phase_stops_when_the_retry_budget_is_spent`.
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

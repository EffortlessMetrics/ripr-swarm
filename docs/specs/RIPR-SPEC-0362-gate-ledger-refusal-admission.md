# RIPR-SPEC-0362: A saved producer-refusal envelope is refused at gap-ledger and gate admission, never admitted as a benign empty ledger

Status: proposed

Owner: product / output

Created: 2026-10-27

Linked proposal:

- None yet

Linked ADRs:

- None yet

Linked plan:

- None yet

Linked issues:

- #10733

Linked PRs:

- None yet

Support-tier impact:

- No tier change. Gate and ledger behavior stays fail-closed; a saved refusal
  now keeps its refusal instead of degrading into a pass. Claim boundaries
  remain governed by [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `policy/doc-artifacts.toml` (via
  `policy/doc-artifacts.d/`) and `.ripr/traceability.d/`.
- No output schema change. `docs/OUTPUT_SCHEMA.md` documents the admission
  rule alongside the existing bounded/partial refusals.

## Problem

`ripr check` refuses an oversized diff before analysis with exit 2, zero
findings, `run_limitations` naming `diff_scope_oversized` or
`repo_scope_oversized`, and `downstream_consumable: false` in the limited
artifact. Saving that artifact and feeding it downstream used to launder the
refusal into success:

1. `ripr reports gap-ledger --check-output <refusal.json>` admitted the
   envelope as a benign `no_records` ledger — `run_limitations` was copied,
   `analysis_scope` was dropped, no warning named the refusal.
2. `ripr gate evaluate` then acknowledged the empty ledger: an
   acknowledgeable mode passed on the empty denominator, exit 0.

A refused run therefore flowed through the saved-record pipeline as a green
gate, the opposite of the producer's verdict. The bounded and partial typed
outcomes were already refused at both admissions; the plain refusal envelope
was the missing case.

## Behavior

### Refusal identity

A document is a **producer-refusal envelope** when its run state carries a
refusal status — `diff_scope_oversized` or `repo_scope_oversized` in the
standard run-status positions — and its `analysis_scope` or
`run_limitations` disclose `downstream_consumable: false` explicitly. An
explicit `false` is required: an absent flag is the healthy complete shape
and never counts as a refusal. A document that discloses
`downstream_consumable: false` under a status this reader does not know is
still refused, naming the generic flag rather than a typed status.

### Ledger admission

`reports gap-ledger --check-output` lands a producer-refusal envelope on
`blocked` with a warning naming the typed refusal status (or the generic
non-consumable flag), instead of the healthy `no_records` state.
`run_limitations` keeps propagating unchanged. Bounded and partial documents
keep their existing ledger handling, and a complete zero-finding envelope
keeps its healthy `no_records` ledger.

### Gate admission

`gate evaluate` refuses a producer-refusal envelope — and any document
disclosing a `downstream_consumable: false` record under an unknown typed
status — at all three inputs (pr-guidance, gap-ledger, baseline) with a
`config_error` naming the reason, exactly like the bounded and partial
refusals. The new checks run after the existing typed bounded/partial/
incomplete checks so those keep their named repairs. Exit status is
`config_error` (exit 2), never an acknowledgeable pass on the empty
denominator.

## Required Evidence

- The refusal-identity predicates (`refusal_run_status`,
  `discloses_non_consumable_record`, `refusal_input_error`) and their wiring
  into all three gate input readers.
- The ledger-side refusal landing (`blocked` + named warning) for both
  refusal statuses.
- The healthy controls: complete zero-finding stays `no_records`/pass,
  bounded stays refused by its own `limited_findings_bound` check.

## Known Limits

- The refusal-status vocabulary is the run-state positions this reader
  knows; a refusal status stored in a non-standard position is caught only
  by the generic `downstream_consumable: false` check, which names the flag
  rather than the typed status.
- Advisory-mode gate evaluations keep their advisory semantics; the refusal
  is refused as a `config_error` input, not promoted to a candidate.

## Non-Goals

- Re-running or repairing the refused analysis.
- Extending the refusal vocabulary beyond the producer's typed statuses.
- Changing bounded/partial/incomplete admission, which keeps its existing
  named repairs.

## Acceptance Examples

1. A saved `diff_scope_oversized` refusal envelope through
   `reports gap-ledger --check-output` lands on `blocked` with a warning
   naming `diff_scope_oversized`; the resulting ledger through
   `gate evaluate` is a `config_error` (exit 2), not a pass.
2. The same envelope fed directly to `gate evaluate` at the pr-guidance,
   gap-ledger, or baseline input is refused with a `config_error` naming
   the reason.
3. A complete zero-finding envelope keeps its `no_records` ledger and its
   acknowledgeable gate pass; a bounded envelope is still refused by its
   own `limited_findings_bound` check.
4. A document disclosing `downstream_consumable: false` under an unknown
   status is refused naming the generic flag; a document with no explicit
   `false` is never treated as a refusal.

## Test Mapping

- `crates/ripr/src/output/gate/tests.rs`: `gate_pr_guidance_refuses_producer_refusal_run`,
  `gate_baseline_refuses_producer_refusal_envelope`,
  `gate_gap_ledger_refuses_generic_non_consumable_record`,
  `gate_refuses_saved_refusal_ledger_with_named_reason`,
  `gate_refusal_admission_keeps_complete_and_advisory_controls`,
  `producer_refusal_detection_covers_run_state_vocabulary`,
  `non_consumable_record_detection_requires_explicit_false`.
- `crates/ripr/src/output/gap_decision_ledger.rs`:
  `gap_decision_ledger_refuses_diff_scope_oversized_refusal_envelope`,
  `gap_decision_ledger_refuses_repo_scope_oversized_refusal_envelope`,
  `gap_decision_ledger_refusal_admission_keeps_healthy_controls`.

## Implementation Mapping

- `crates/ripr/src/output/gate.rs`: the shared refusal-identity predicates.
- `crates/ripr/src/output/gate/input.rs`: reader wiring after the existing
  typed checks.
- `crates/ripr/src/output/gap_decision_ledger.rs`: the ledger-side refusal
  landing.

## Metrics

- `unit_test_pass_rate`: the tests above are required Rust tests.

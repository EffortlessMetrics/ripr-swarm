# RIPR-SPEC-0152: Candidate actionability convergence

Status: proposed

Issue: #3281 (parent #3212; producer prerequisite #3280)

## Problem

A finding classification does not establish that the recorded source belongs to
the candidate revision. Before this contract, output surfaces independently
interpreted class, path, line, and repair prose. A deleted-side finding could
therefore disappear from guidance while still contributing to a blocking count,
head diagnostic, repair card, mutation route, badge, gate, or agent packet.

RIPR-SPEC-0151 gives every finding one producer-owned
`source_currentness` disposition. This contract makes that disposition the
single source-eligibility authority for every actionability-bearing projection.

## Behavior

`SourceCurrentness` maps to one crate-owned candidate-actionability state:

```text
candidate_current   -> candidate_eligible
base_deleted        -> historical_deleted
moved_or_renamed    -> movement_unresolved
unresolved_subject  -> subject_unresolved
```

`candidate_eligible` is necessary but not sufficient for actionability. Existing
classification, policy, suppression, repair-route, packet-validation, and
verification rules still apply. The other three states are evidence-only and
cannot authorize a candidate edit target, current diagnostic, blocking gap,
mutation route, review action, or agent action.

The primary check JSON remains the evidence archive. Every finding carries both
`source_currentness` and a structured `candidate_actionability` object with
`status`, `eligible`, `edit_target`, `revision_context`, and `reason`. The
summary carries the candidate-current denominator, candidate-actionable count,
evidence-only disposition counts, and candidate-current class buckets. Existing
raw class buckets remain evidence inventory and are not candidate-actionability
authority.

Actionability-bearing projections consume the shared authority as follows:

- human output counts and repair guidance use only unsuppressed,
  candidate-current findings; non-current evidence is labelled by revision and
  receives no candidate repair command;
- finding alignment and the gap decision ledger exclude non-current findings;
  gate, baseline/RIPR Zero, badge, review, and agent projections inherit that
  decision through the ledger rather than re-reading raw findings;
- Python, TypeScript/JavaScript, and Perl repair cards or preview packets cannot
  materialize for non-current findings;
- GitHub output emits no file/line annotation for non-current evidence and emits
  one aggregate evidence-only notice;
- LSP diagnostics, code lenses, and stale-action revalidation omit non-current
  findings from candidate source;
- SARIF retains non-current evidence at `note` level, without `locations`, and
  carries the structured actionability reason in result properties;
- diff reports retain the evidence item with its revision context and a
  non-repair next step, while candidate class and actionability counts exclude
  it;
- context packet version `1.1` carries source currentness and structured
  candidate actionability; non-current packets have `edit_target: false`;
- PR-evidence routing consumes the candidate-actionability summary and fails
  closed when that authority is absent. Targeted mutation candidates are built
  only from `candidate_current` findings.

Missing, malformed, unknown, or pre-contract wire values map to
`subject_unresolved`. Consumers must not recover currentness from file paths,
line numbers, finding classes, severity, canonical-gap identity, or prose.

## Required Evidence

- The domain mapping proves that only `candidate_current` crosses the candidate
  edit boundary and that every controlled wire value has a named revision
  context and reason.
- Primary JSON proves one mixed finding set yields one candidate-current
  actionable count and three evidence-only records without changing the
  evidence archive.
- Finding alignment and the gap ledger reject deleted, moved, unresolved, and
  legacy/missing-currentness records.
- Human, JSON, SARIF, GitHub, diff, context-packet, badge, and editor tests pin
  the same denominator and non-repair behavior.
- A mixed current/non-current PR-evidence input keeps the real current mutation
  candidate and ignores the historical record.
- Removing the shared source-eligibility check from a representative repair,
  ledger, annotation, or editor projection makes its behavioral test fail.
- Candidate-current controls retain their prior class, repair, and projection
  behavior except for reviewed additive actionability fields and schema-version
  updates where the public object shape changed.

## Required Guards

- No renderer-specific path, line, class, severity, or prose heuristic may
  upgrade a non-current finding.
- Suppression composes after candidate eligibility for actionable counts; a
  suppressed current finding does not reappear through the new denominator.
- Historical evidence may remain visible only on surfaces that identify its
  revision context and do not expose a candidate edit target.
- `moved_or_renamed` and `unresolved_subject` remain limitations until a producer
  proves candidate identity; consumers do not guess a new range.
- Legacy check output without candidate-actionability summary cannot trigger PR
  mutation routing.

## Acceptance Examples

- Accept: one `base_deleted` weak finding remains in check JSON and SARIF, but
  contributes zero current actionable gaps, no head annotation, no LSP
  diagnostic, no repair card, and no ledger record.
- Accept: one `moved_or_renamed` finding renders `revision_context: unresolved`
  and a controlled source-identity next step, with no guessed candidate range.
- Accept: a mixed diff with one candidate-current weak finding and one deleted
  weak finding reports one candidate actionable item and retains both evidence
  records.
- Reject: a legacy finding with no `source_currentness` becoming actionable
  because it is `weakly_exposed` or has a source-like file and line.

## Test Mapping

- `crates/ripr/src/domain/probe.rs::source_currentness_tests::only_candidate_current_crosses_the_candidate_edit_boundary`
- `crates/ripr/src/output/candidate_actionability.rs::tests::json_currentness_resolution_is_controlled_and_fail_closed`
- `crates/ripr/src/output/json/mod.rs::tests::candidate_actionability_summary_and_findings_share_one_denominator`
- `crates/ripr/src/output/json/finding_alignment.rs::tests::non_candidate_findings_do_not_enter_canonical_actionability_projection`
- `crates/ripr/src/output/gap_decision_ledger.rs::tests::check_output_decision_ledger_rejects_non_candidate_findings`
- `crates/ripr/src/output/badge/tests.rs::badge_summary_excludes_non_candidate_findings_from_headline`
- `crates/ripr/src/output/github.rs::tests::render_non_candidate_findings_as_global_evidence_notice_only`
- `crates/ripr/src/output/sarif.rs::tests::sarif_retains_non_candidate_evidence_without_a_head_location`
- `crates/ripr/src/output/diff_report.rs::tests::diff_report_keeps_non_candidate_evidence_without_counting_or_targeting_it`
- `crates/ripr/src/output/next_step.rs::tests::reconcile_next_step_non_candidate_overrides_repair_prose`
- `crates/ripr/src/lsp/diagnostics.rs::diagnostic_policy_tests::non_candidate_findings_are_never_visible_as_head_diagnostics`
- `crates/ripr/src/lsp/lens.rs::tests::code_lens_and_view_identity_exclude_non_candidate_evidence`

## Non-Goals

This contract does not infer source currentness, change recorded finding
coordinates, resolve preview-language currentness, lower classification
thresholds, change release membership, or remove downstream containment for the
complete #3212 reproducer. Producer inference remains RIPR-SPEC-0151/#3280;
end-to-end downstream closeout remains C3.

## Implementation Mapping

- Domain authority: `crates/ripr/src/domain/probe.rs`
- Shared output projection: `crates/ripr/src/output/candidate_actionability.rs`
- Evidence archive and canonical grouping:
  `crates/ripr/src/output/json/report.rs`,
  `crates/ripr/src/output/json/finding_alignment.rs`
- Decision authority: `crates/ripr/src/output/gap_decision_ledger.rs`
- Human and machine surfaces: `crates/ripr/src/output/human.rs`,
  `crates/ripr/src/output/diff_report.rs`,
  `crates/ripr/src/output/github.rs`, `crates/ripr/src/output/sarif.rs`,
  `crates/ripr/src/output/json/context_packet.rs`
- Repair projections: `crates/ripr/src/output/python_repair_card.rs`,
  `crates/ripr/src/output/typescript_packet_projection.rs`,
  `crates/ripr/src/output/typescript_preview_card.rs`,
  `crates/ripr/src/output/perl_preview_card.rs`
- Editor boundary: `crates/ripr/src/lsp/diagnostics.rs`,
  `crates/ripr/src/lsp/lens.rs`, `crates/ripr/src/lsp/actions.rs`
- Downstream PR route: `crates/ripr/src/app/pr_evidence.rs`,
  `xtask/src/reports/pr_evidence.rs`

## Metrics

The candidate-actionability summary reports candidate-current,
candidate-actionable, and evidence-only counts. Raw finding totals and raw class
inventory remain available for evidence accounting. A projection is conforming
only when its actionable denominator reconciles to the shared candidate-current
set after suppression and existing class/policy rules.

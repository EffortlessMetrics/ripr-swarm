# RIPR-SPEC-0146: Release supplemental denominator ledger

Status: accepted

Issue: #2768

Related release-control work: #2767, #1609, #2379

## Intent

The supplemental denominator report records every commit in a captured
first-parent release range and gives each record an explicit reviewed
disposition. It is a reconciliation aid for the 0.11 release process, not a
candidate qualification or publication decision.

## Input contract

`release-denominator --input <ledger.json>` accepts a versioned
`release_denominator_snapshot` containing the historical base, candidate ref
and SHA, ordered range commits, candidate-tree commits, and one record per
range commit. Each record carries release disposition, ownership, tree state,
review/proof references, and source-survivor or swarm-exclusion context.

The accepted dispositions are `include_product`,
`include_release_infrastructure`, `include_control_or_honesty`,
`structural_no_semantic_delta`, `candidate_only_exclusion`,
`source_only_followup`, `safe_defer_post_0_11`, and
`operator_decision_required`. The report uses `ready` or
`reconcile_required`; it never claims that a candidate is qualified, published,
or released.

`--live --input <ledger.json>` additionally compares the captured candidate
SHA, first-parent range, and candidate-tree commit set with bounded live Git
observations. It does not replace the reviewed ledger.

## Validation

Validation fails closed for missing, duplicate, out-of-range, or wrongly
ordered records; range/tree disagreement; disposition/tree-state mismatch;
unresolved operator decisions in a final ledger; and live observation drift.
JSON and Markdown are rendered from the same normalized report and carry the
same authority boundary and limitation statements.

## Outputs

The command writes:

- `target/ripr/reports/release-denominator.json`
- `target/ripr/reports/release-denominator.md`

The report includes stable range, candidate-tree, and record-set digests,
record counts, disposition/tree-state counts, reconciliation reasons, and the
next action. Provisional output identifies the missing final candidate
decision; final output is ready only after every record is reconciled.

## Acceptance and proof map

Acceptance is limited to a deterministic supplemental denominator ledger and a
shared fail-closed validator. It does not close the final candidate decision
tracked by #1609 or the dependent release-editor lane #2769.

The implementation and fixture contract are mapped in `.ripr/traceability.toml`
under `RIPR-SPEC-0146`. Focused proof is provided by the nine tests named there
and the complete/reconcile-required fixtures under
`fixtures/release_denominator/`; hosted CI is the authoritative execution
proof for this PR.

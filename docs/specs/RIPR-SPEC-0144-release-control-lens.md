# RIPR-SPEC-0144: Temporary 0.11 Release-Control Lens

Status: accepted

Owner: release control / swarm operations

Created: 2026-07-29

Linked issues:

- [#2766](https://github.com/EffortlessMetrics/ripr-swarm/issues/2766) — bind
  0.11 work selection and merge eligibility to the live #2379 graph.
- [#2379](https://github.com/EffortlessMetrics/ripr-swarm/issues/2379) — exact
  0.11 candidate authority.

Support-tier impact:

- No product support-tier change. This is a maintainer-facing, read-only
  control-plane report and cannot strengthen analyzer, release, or merge claims.
- Reference: [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- No release publication, source integration, version, credential, workflow,
  dependency, or secret change.

## Problem

The 0.11 release authority and its writer cutoff are currently expressed in
issue prose, while the live repository continues to receive independent work.
An open-ended session can therefore select a useful PR outside the accepted
release graph without recording whether that PR is required, deferred, or
blocked for the candidate. A green check on such a PR is not a release
disposition.

## Behavior

`cargo xtask release-control --input <snapshot.json>` replays a captured,
schema-versioned snapshot. `cargo xtask release-control --live` collects the
current `origin/main`, open PR inventory, and #2379 state through bounded
read-only adapters. Both paths produce a snapshot containing the observed
`main` SHA, #2379 authority, portfolio and active-claim completeness, and every
open PR. The command sorts PR records by number and derives both JSON and
Markdown from one normalized report.

Every PR must carry one of these dispositions:

- `release_required`;
- `release_optional_pending_decision`;
- `hold_post_release`;
- `blocked_on_named_authority`.

Only a complete, current snapshot can produce `status = ready`, and only a
non-draft `release_required` row can be `merge_eligible = true`. Missing,
stale, contradictory, duplicated, or malformed authority input produces
`status = reconcile_required` and clears merge eligibility for every row.
Missing per-PR disposition defaults visibly to
`blocked_on_named_authority` while retaining the reconciliation reason.

The report is advisory and report-only. It does not close issues, relabel
items, merge or rebase PRs, create or delete branches, select a candidate,
qualify a release, or mutate development `main`.

## Input contract

The input envelope has `schema_version = "0.1"` and
`kind = "release_control_snapshot"`. Captured input must use
`source.mode = "captured"`; `source.mode = "live"` is admitted only for the
internal `--live` collector, not for an input file. `source` must identify the
current main SHA, open #2379 state, matching authority/main identity, complete
portfolio and claim inventory observations, worktree inventory, current
freshness, and a non-empty graph digest. The live collector records worktree
inventory but deliberately leaves authority-main identity, portfolio
completeness, and active-claim completeness unresolved because its bounded
inputs do not prove those facts; the resulting report is therefore
`reconcile_required` until an approved source supplies them. PR rows carry a
number, title, open state, head SHA, `main` base ref, and explicit
disposition/reason.

The fixture corpus in `fixtures/release_control/` is manifest-only and is
validated by `cargo xtask check-fixture-contracts`. It includes a complete
snapshot and a stale/incomplete snapshot that must reconcile.

## Output contract

The command writes `target/ripr/reports/release-control.json` and
`target/ripr/reports/release-control.md`. JSON is schema `0.1` and contains the
normalized source observation, sorted PR rows, `reconciliation_reasons`, a
`status` of `ready` or `reconcile_required`, per-row `merge_eligible`, a
`next_action`, and the explicit `authority_boundary`/`must_not_claim` fields.
Markdown is a projection of that same normalized DTO; it cannot strengthen a
reconciliation-required state or any per-PR disposition.

## Required Evidence

- `xtask/src/reports/release_control.rs` owns snapshot parsing, bounded live
  collection, fail-closed normalization, and the shared JSON/Markdown DTO
  projection.
- `fixtures/release_control/complete.json` and
  `fixtures/release_control/reconcile-required.json` cover current complete
  and stale/incomplete authority inputs.
- `cargo xtask check-fixture-contracts` and `cargo xtask check-output-contracts`
  validate the fixture and output shape.
- The focused test corpus covers deterministic ordering, malformed authority
  and PR fields, live collector success/failure, bounded subprocess errors,
  and the explicit claim boundary.

## Acceptance Examples

- fixed captured inputs produce byte-stable normalized JSON and Markdown;
- PR input order cannot change the normalized report;
- unrelated work remains visible and non-mergeable;
- live collection inventories current main/open PR/#2379 inputs;
- missing or stale authority never becomes merge eligibility;
- the report preserves an explicit claim boundary and next action;
- the command performs no external state mutation.

## Proof

```text
cargo test -p xtask release_control -- --nocapture
cargo xtask release-control --input fixtures/release_control/complete.json
cargo xtask release-control --live
cargo xtask check-output-contracts
cargo xtask check-fixture-contracts
cargo xtask check-pr
```

## Non-Goals

- no singleton active-goal authority or automatic backlog priority;
- no candidate denominator, exact-candidate qualification, package proof,
  source handoff, version bump, tag, publication, signing, or marketplace;
- no issue closure, merge queue, branch operation, or GitHub mutation;
- no replacement for #2379, #1609, #1704, or #1706.

## Claim boundary

This spec proves only that a captured input or bounded live observation is
normalized into an explicit, deterministic, fail-closed disposition report. It
does not prove that a PR is correct, that a release candidate is qualified, or
that a merge is approved.

## Test Mapping

- `xtask/src/reports/release_control.rs::tests::complete_snapshot_is_ready_and_only_required_rows_are_merge_eligible`
  — complete authority input produces one eligible required row and keeps
  held work visible.
- `xtask/src/reports/release_control.rs::tests::input_order_does_not_change_normalized_output`
  — JSON and Markdown projections are deterministic under PR reordering.
- `xtask/src/reports/release_control.rs::tests::malformed_source_and_pr_fields_all_fail_closed`
  — malformed authority and PR fields produce reconciliation reasons and no
  eligible rows.
- `xtask/src/reports/release_control.rs::tests::bounded_live_collector_normalizes_success_and_failure_inputs`
  — bounded live collection records successful and failed adapter inputs.
- `xtask/src/reports/release_control.rs::tests::live_command_and_result_interpretation_are_bounded`
  — subprocess success, failure, missing program, timeout, and missing status
  retain actionable diagnostics.

## Implementation Mapping

- `xtask/src/command.rs`, `xtask/src/dispatch.rs`, and
  `xtask/src/reports/mod.rs` expose the report-only command.
- `xtask/src/reports/release_control.rs` implements normalization and output.
- `xtask/src/fixture_contracts/mod.rs` validates the release-control fixture
  corpus.
- `docs/OUTPUT_SCHEMA.md` documents the JSON and Markdown report shape.
- `plans/release-control-0-11/implementation-plan.md` sequences the dependent
  execution-scope, denominator, and candidate-bundle slices.

## Metrics

- `status` counts ready versus reconciliation-required snapshots.
- `prs[].merge_eligible` counts rows admitted by this temporary lens only.
- `source.open_prs_complete`, `source.active_claims_complete`, and
  `source.worktree_inventory_complete` preserve completeness denominators.
- `reconciliation_reasons` and `next_action` make missing authority evidence
  actionable without converting unknown state into a release claim.

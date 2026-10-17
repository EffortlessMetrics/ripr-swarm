# RIPR-SPEC-0278: Issue lifecycle real closeout rows pilot over two captured journeys

Status: proposed

Owner: test-infra

Created: 2026-10-17

Linked issues:

- #4932 (this slice: adapt one real issue-to-merge journey into burn-down
  and current-head closeout evidence)
- #1650 (parent/controller: governed issue-to-closeout acceptance)
- #4929 / RIPR-SPEC-0218 (the frozen attempt contract and counting law this
  pilot consumes; referenced, never duplicated)
- #4930 / RIPR-SPEC-0223 and #4931 / RIPR-SPEC-0232 (the sibling intake and
  contract/plan pilots over real captured snapshots)

Support-tier impact:

- None.
- The pilot is an offline typed projection over committed, digest-bound
  GitHub captures; it launches no agent, selects no work, edits nothing,
  calls no provider and reads no live GitHub state at validation time.

Policy impact:

- None. No new process, network or file-policy surface; the corpus and
  validators read committed fixtures only. Two verbatim capture files keep
  reasoned `check-static-language` allowlist entries mirroring the
  RIPR-SPEC-0223 intake-pilot precedent.

## Problem

RIPR-SPEC-0218 landed the issue-lifecycle attempt contract, the
merge-versus-closeout downgrade law and the deterministic scorecard over
thirteen synthetic mechanics scenarios — with zero real rows, the projected
implementation success rate stayed honestly `not_measured`. #4932 relaxes
the original #1707 journey dependency (premise repair, 2026-10-17) to any
equivalently-evidenced landed journey. Before the machinery is trusted on
live work, it needs one honest exercise over exactly two REAL captured
journeys: can one complete issue-to-merge closeout count as `completed`
with merge, current-head verification, acceptance coverage, burn-down and
closeout as distinct bound identities — and does one merged PR whose parent
stays open with uncovered acceptance stay `merged_pending_closeout` no
matter what the producer claims?

## Behavior

`fixtures/issue_lifecycle_real_rows/` commits two real
(`synthetic: false`) `issue_lifecycle_attempt.v1` rows plus the immutable
gh capture bytes under `snapshots/`, a `provenance.json` (capture
timestamp, base main SHA, the exact read-only verification commands and the
LF-normalized sha256 of every snapshot) and a `controls.json` coverage map
for the ten #4932 controls.

Row A binds the complete journey of issue #7795 (mixed-quote rename
disclosure): issue snapshot -> PR #8285 -> green checks on published head
`1b4a5f651b1bac1b7bf789858ad1e591ca8061be` -> squash merge
`f3fa41d265e1fb51f280d6d8d5f0c4967c362025` -> issue CLOSED one second
after the merge -> current-head verification by compare against main
`b038650091ca6183e252e459b91c4aff35eef701` (`behind_by: 0`) -> burn-down
closed with all five acceptance predicates covered -> closeout completed.
Facts that GitHub does not record are not invented: no formal claim record
and no #4925 base orchestration attempt receipt exist for the lane, and no
human approving review is on record — each gap stays in the row's
`limitations`.

Row B binds the live negative control: PR #8325 (issue #1706 delivery
slice PR B) squash-merged as `70d053fb7b4a32c5855fb62cf30249e91752f3be` on
2026-10-17 while parent issue #1706 stays OPEN (`status/needs-work`) with
all ten acceptance rows uncovered and no current-head receipt bound. The
row claims and keeps `merged_pending_closeout`: merge alone cannot close.

`cargo xtask check-fixture-contracts` recomputes every snapshot digest
fail-closed over LF-normalized bytes (matching the LF bytes git stores per
`.gitattributes * text=auto eol=lf`), rebinds each row digest through the
unchanged RIPR-SPEC-0218 counting law, and rejects any drift between the
corpus, the provenance and the captures. It also cross-checks capture-time
consistency: `provenance.captured_at` must not predate the latest
`mergedAt`/`closedAt` event any bound snapshot records, so a provenance
claiming to capture evidence before that evidence existed rejects instead
of passing silently. The adapter tests in
`cargo test -p xtask issue_lifecycle_real_rows` pin both rows loading and
counting, Row A dispositions through to completed closeout, Row B staying
`merged_pending_closeout`, wrong-head/wrong-merge identity rejection on the
retained digest binding, zero-subject verification demoting a claimed
completed closeout, stale current-main receipts forcing `stale`, uncovered
acceptance demoting a merge to `partially_landed`, duplicate import
deduplication by `observation_key`, and the thirteen synthetic mechanics
fixtures staying outside every real denominator while Row A remains
`completed` beside pending Row B.

## Test Mapping

- `issue_lifecycle_real_rows::tests::both_real_rows_load_with_intact_provenance_bindings`
- `issue_lifecycle_real_rows::tests::row_a_dispositions_through_to_completed_closeout`
- `issue_lifecycle_real_rows::tests::row_b_stays_merged_pending_closeout`
- `issue_lifecycle_real_rows::tests::wrong_head_or_merge_identity_variant_rejects`
- `issue_lifecycle_real_rows::tests::zero_subject_verification_cannot_close`
- `issue_lifecycle_real_rows::tests::missing_or_stale_current_head_proof_blocks_closeout`
- `issue_lifecycle_real_rows::tests::uncovered_acceptance_row_keeps_a_merge_partial`
- `issue_lifecycle_real_rows::tests::duplicate_import_deduplicates_by_observation_key`
- `issue_lifecycle_real_rows::tests::synthetic_fixtures_stay_outside_the_real_count`

## Implementation Mapping

- `xtask/src/issue_lifecycle_real_rows.rs` — provenance DTOs, LF-normalized
  snapshot digest binding, corpus/provenance loaders, adapter tests above.
- `xtask/src/issue_lifecycle_attempt.rs` — the reused RIPR-SPEC-0218
  counting law, disposition vocabulary and row digest identity
  (referenced, never forked).
- `xtask/src/reports/issue_lifecycle.rs` — the reused scorecard projection
  and deduplication law.
- `xtask/src/fixture_contracts/mod.rs`,
  `xtask/src/fixture_contracts/general_validators.rs`,
  `xtask/src/reports/fixtures.rs`, `xtask/src/main.rs` — wiring and
  manifest-only registration.

## Metrics

- unit_test_pass_rate
- fixture corpus pass rate (pinned at 100% by `check-fixture-contracts`
  over LF-normalized snapshot digests)
- real lifecycle count (2) and implementation success rate (1/2 measured)
  as projected by `issue-lifecycle-scorecard` over the committed corpus

## Required Evidence

- Committed corpus: two real rows, digest-bound provenance, ten-control
  coverage map, immutable snapshot bytes, rerun fail-closed by
  `check-fixture-contracts` over LF bytes.
- `cargo xtask issue-lifecycle-scorecard --captured fixtures/issue_lifecycle_real_rows/corpus.json`
  projects two real lifecycles and one implementation success (1/2
  measured) with synthetic mechanics rows counted separately.
- Adapter tests: `cargo test -p xtask issue_lifecycle_real_rows`.
- Capture commands: recorded verbatim in `provenance.json`
  (`gh issue view`, `gh pr view`, `gh api` compare/branches/reviews);
  zero GitHub mutation.

## Acceptance Examples

- Row A (`lifecycle-real-7795-8285-closeout`) counts as one real lifecycle
  at `completed` with merge `f3fa41d265e1fb51f280d6d8d5f0c4967c362025`,
  current-head verification against main
  `b038650091ca6183e252e459b91c4aff35eef701`, five covered acceptance
  predicates and a completed closeout — no downgrade reason retained.
- Row B (`lifecycle-real-8325-1706-pending-closeout`) counts as one real
  lifecycle at `merged_pending_closeout` with merge
  `70d053fb7b4a32c5855fb62cf30249e91752f3be` bound, no current-head
  receipt and the ten uncovered #1706 acceptance rows visible.
- A Row A variant with a fabricated wrong head or wrong merge identity and
  no restamped digest rejects on the row_digest binding.
- A Row A variant with stripped verification subjects demotes to
  `merged_pending_closeout`; a stale current-main receipt forces `stale`;
  an uncovered acceptance row demotes the merge to `partially_landed`.
- Importing the committed corpus twice deduplicates to two lifecycles with
  two observations each; the thirteen synthetic mechanics fixtures stay
  outside every real denominator.

## Non-Goals

- No new real PR, issue closure, source edit, merge or closeout; the rows
  capture evidence that already exists.
- No portfolio architecture, role adapter, policy ratification or
  broad-lifecycle accuracy claim.
- No #1707 telemetry fabrication: control 10 (missing #1707 telemetry)
  stays `not_measured`.

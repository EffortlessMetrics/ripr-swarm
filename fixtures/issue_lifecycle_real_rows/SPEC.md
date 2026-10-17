# Issue Lifecycle Real Rows Corpus

Spec: RIPR-SPEC-0278

## Given

The RIPR-SPEC-0218 counting law and scorecard machinery landed with thirteen
synthetic mechanics scenarios and zero real rows. Issue #4932 (premise
repaired 2026-10-17) admits the first real issue-to-merge journey into the
lifecycle corpus: one COMPLETE closeout journey plus one captured NEGATIVE
CONTROL proving that merge alone cannot close. Both rows are real
(`synthetic: false`) and every bound identity was verified against GitHub
(`EffortlessMetrics/ripr-swarm`) with read-only `gh` commands at capture
time 2026-10-17; the exact commands are recorded in `provenance.json` and
the raw captures are committed under `snapshots/`.

Row A binds the complete journey of issue #7795 (mixed-quote rename
disclosure): issue snapshot -> PR #8285 -> green checks on head
`1b4a5f651b1bac1b7bf789858ad1e591ca8061be` -> squash merge
`f3fa41d265e1fb51f280d6d8d5f0c4967c362025` -> issue CLOSED ->
current-head verification (compare against main
`b038650091ca6183e252e459b91c4aff35eef701`, `behind_by: 0`) -> burn-down
closed with all five acceptance predicates covered -> closeout completed.

Row B binds the negative control of PR #8325 (issue #1706 delivery slice
PR B): squash merge `70d053fb7b4a32c5855fb62cf30249e91752f3be` on
2026-10-17 while parent issue #1706 remains OPEN with all ten acceptance
rows uncovered -> disposition `merged_pending_closeout` with the missing
current-head receipt and the uncovered rows stated.

## When

An offline validator run loads `corpus.json` (plain
`issue_lifecycle_corpus.v1` shape), recomputes every `row_digest` binding
over the canonical retained surface, and assesses each row through the
unchanged RIPR-SPEC-0218 counting law. `provenance.json` is checked
against the corpus: row digests, observation keys, lifecycle ids and the
pr/merge/issue identities must agree. Every snapshot digest recorded in
`provenance.json` and in the rows' intake evidence is recomputed over
LF-normalized bytes (matching the LF bytes git stores per `.gitattributes
* text=auto eol=lf`), so a CRLF rewrite or a content edit rejects the row.

## Then

- Row A counts as exactly one real lifecycle at disposition `completed`:
  merge, current-head verification, acceptance coverage, burn-down and
  closeout are distinct, bound identities.
- Row B counts as exactly one real lifecycle at disposition
  `merged_pending_closeout`: the merge identity is bound, the closeout
  state stays `pending`, `current_head_verification` stays absent, and the
  ten uncovered #1706 acceptance rows stay visible.
- Both rows are real: the projected scorecard reports two real lifecycles
  and one implementation success (1/2 measured); the thirteen synthetic
  mechanics fixtures of `fixtures/issue_lifecycle_attempts` never enter
  the real denominator.
- Unknown stays unknown: no formal claim record and no #4925 base
  orchestration attempt receipt exists for either lane; both facts are
  recorded in each row's `limitations` instead of being invented.

## Must Not

- Do not count a synthetic mechanics row in the real denominator.
- Do not invent claim, review, or base-attempt identities that GitHub does
  not record; record the gap as a limitation.
- Do not let a merge identity imply a completed closeout (Row B stays
  `merged_pending_closeout`).
- Do not upgrade a claimed disposition or hide downgrade reasons.
- Do not mutate GitHub state during capture or validation.
- Do not recompute snapshot digests over CRLF bytes; LF normalization is
  part of the recorded identity.

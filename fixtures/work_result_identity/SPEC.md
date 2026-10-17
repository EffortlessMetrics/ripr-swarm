# Selected-work task-result corpora (RIPR-SPEC-0269, #1706)

Corpus for the task/result/receipt schema consumer in
`xtask/src/work_result_identity.rs` (#1706, delivery slice PR B, consuming
#1633). The check validates committed `WorkTaskResultV1` rows against a
`PortfolioBasisV1` compiled from immutable captured inputs; it selects no
work, claims nothing, grants no authority and mutates nothing.
`check-fixture-contracts` recomputes the provenance digests fail-closed.

## Layout

- `corpus.json` — the five required scenarios. Each scenario names the
  captured portfolio directory it binds against (referenced, never
  duplicated: a `fixtures/work_portfolio` corpus or a RIPR-SPEC-0244
  `fixtures/work_selection_identity/captured/` variant) and one or more
  result cases. A case carries a `WorkTaskResultV1` row, the expected
  outcome (`pass`/`fail`) and the exact expected law/route pairs for
  failures. Every result embeds the full `SelectedWorkIdentityV1` it
  extends, so the RIPR-SPEC-0244 selection laws run inside the result
  check before the result-level laws.
- `provenance.json` — per-file SHA-256 bindings in the RIPR-SPEC-0223
  provenance convention.

## The five scenarios

1. `result-01-one-campaign-selection-results-pass` — context, build and
   review results bind a selection in one campaign.
2. `result-02-standalone-selection-results-pass` — a standalone selection
   with zero campaigns still carries results.
3. `result-03-repair-selection-worktree-result-passes` — a repair result
   binds the worktree the selection recorded (`<root>/wt-9101`).
4. `result-04-legacy-read-only-migration-posture` — legacy refs on a
   result resolve read-only as a historical campaign reference and emit
   migration posture with no authority.
5. `result-05-wrong-identities-fail-stale-or-wrong-input` — wrong
   repository, portfolio snapshot, basis, reported head, worktree,
   embedded action, missing evidence and an unresolvable legacy ref each
   fail with their exact law/route pair.

## Regeneration

`corpus.json` is authored by the PR B delivery script against the
committed RIPR-SPEC-0244 selection corpus; edit that corpus and this one
together, then refresh `provenance.json` digests.

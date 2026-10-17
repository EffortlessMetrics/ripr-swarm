# Selected-work burn-down corpora (RIPR-SPEC-0274, #1706)

Corpus for the burn-down consumer in
`xtask/src/work_burndown_identity.rs` (#1706, delivery slice PR B,
consuming #1648). The check validates committed `WorkBurnDownV1` rows
against a `PortfolioBasisV1` compiled from immutable captured inputs; it
selects no work, claims nothing, grants no authority and mutates nothing.
`check-fixture-contracts` recomputes the provenance digests fail-closed.

## Layout

- `corpus.json` — the five required scenarios. Each scenario names the
  captured portfolio directory it binds against (referenced, never
  duplicated: a `fixtures/work_portfolio` corpus or a RIPR-SPEC-0244
  `fixtures/work_selection_identity/captured/` variant) and one or more
  burn-down cases. A case carries a `WorkBurnDownV1` row, the expected
  outcome (`pass`/`fail`) and the exact expected law/route pairs for
  failures. Every burn-down embeds the full `SelectedWorkIdentityV1` it
  tracks, so the RIPR-SPEC-0244 selection laws run inside the burn-down
  check before the burn-down-level laws.
- `author_corpus.py` — regenerates `corpus.json` from the committed
  RIPR-SPEC-0244 selection packets (deep-copied; never mutated in place)
  and the captured snapshot identities.
- `provenance.json` — per-file SHA-256 bindings in the RIPR-SPEC-0223
  provenance convention.

## The five scenarios

1. `burndown-01-complete-evidence-burn-down-passes` — a burn-down whose
   evidence rows are all covered, current or merged derives `complete`.
2. `burndown-02-standalone-burn-down-passes` — a standalone selection with
   zero campaigns derives an honest burn-down.
3. `burndown-03-partial-after-one-campaign-item-merges` — burn-down stays
   `partial` after one campaign work item merges while issue acceptance
   remains uncovered; the campaign record stays context only (issue #1706
   fixture 10).
4. `burndown-04-campaign-override-and-wrong-inputs-fail` — a `complete`
   burn-down over an open acceptance row (campaign-record override), a
   wrong current main, an unknown campaign record, an evidence-free
   burn-down and a wrong repository each fail with their exact law/route
   pair.
5. `burndown-05-legacy-read-only-migration-posture` — legacy refs on a
   burn-down resolve read-only as a historical campaign reference and
   emit migration posture with no authority.

## Regeneration

```sh
python fixtures/work_burndown_identity/author_corpus.py
```

After any regeneration, recompute `provenance.json` (sha256 of
`corpus.json`) and rerun `cargo xtask check-fixture-contracts`.

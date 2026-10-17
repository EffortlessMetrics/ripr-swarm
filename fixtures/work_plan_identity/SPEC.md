# Selected-work plan-disposition corpora (RIPR-SPEC-0276, #1706)

Corpus for the work-planning disposition consumer in
`xtask/src/work_plan_identity.rs` (#1706, delivery slice PR B, consuming
#1646). The check validates committed `WorkPlanV1` rows against a
`PortfolioBasisV1` compiled from immutable captured inputs; it selects no
work, claims nothing, grants no authority and mutates nothing. Planning is
read-only: a plan row that would replace or reorder the whole portfolio
fails visibly. `check-fixture-contracts` recomputes the provenance digests
fail-closed.

## Layout

- `corpus.json` — the five required scenarios. Each scenario names the
  captured portfolio directory it binds against (referenced, never
  duplicated: a `fixtures/work_portfolio` corpus or a RIPR-SPEC-0244
  `fixtures/work_selection_identity/captured/` variant) and one or more
  plan cases. A case carries a `WorkPlanV1` row, the expected outcome
  (`pass`/`fail`) and the exact expected law/route pairs for failures.
  Every plan embeds the full `SelectedWorkIdentityV1` it plans for, so the
  RIPR-SPEC-0244 selection laws run inside the plan check before the
  plan-level laws.
- `author_corpus.py` — regenerates `corpus.json` from the committed
  RIPR-SPEC-0244 selection packets (deep-copied; never mutated in place)
  and the captured snapshot identities.
- `provenance.json` — per-file SHA-256 bindings in the RIPR-SPEC-0223
  provenance convention.

## The five scenarios

1. `plan-01-single-scoped-pr-disposition-passes` — a single scoped PR
   disposition for one selected issue passes and binds the snapshot, the
   basis head and derived artifacts.
2. `plan-02-standalone-issue-disposition-passes` — a standalone issue
   disposition without campaign placement passes.
3. `plan-03-append-to-named-campaign-disposition-passes` — an append
   disposition naming a recorded campaign passes.
4. `plan-04-blocked-by-contract-disposition-passes` — a blocked
   disposition naming its blocking contract passes.
5. `plan-05-reorder-and-wrong-inputs-fail` — a portfolio reorder, a
   missing or unknown append target, an unnamed blocker, a foreign
   repository and an unknown legacy goal each fail with their exact
   law/route pair.

## Regeneration

```sh
python fixtures/work_plan_identity/author_corpus.py
```

After any regeneration, recompute `provenance.json` (sha256 of
`corpus.json`) and rerun `cargo xtask check-fixture-contracts`.

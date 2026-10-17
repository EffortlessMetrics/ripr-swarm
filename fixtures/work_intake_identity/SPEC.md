# Selected-work issue-intake corpora (RIPR-SPEC-0275, #1706)

Corpus for the issue intake consumer in
`xtask/src/work_intake_identity.rs` (#1706, delivery slice PR B, consuming
#1644). The check validates committed `WorkIntakeV1` rows against a
`PortfolioBasisV1` compiled from immutable captured inputs; it selects no
work, claims nothing, grants no authority and mutates nothing.
`check-fixture-contracts` recomputes the provenance digests fail-closed.

## Layout

- `corpus.json` — the five required scenarios. Each scenario names the
  captured portfolio directory it binds against (referenced, never
  duplicated: a `fixtures/work_portfolio` corpus or a RIPR-SPEC-0244
  `fixtures/work_selection_identity/captured/` variant) and one or more
  intake cases. A case carries a `WorkIntakeV1` row, the expected outcome
  (`pass`/`fail`) and the exact expected law/route pairs for failures.
- `author_corpus.py` — regenerates `corpus.json`; the captured snapshot
  identities are pinned constants.
- `provenance.json` — per-file SHA-256 bindings in the RIPR-SPEC-0223
  provenance convention.

## The five scenarios

1. `intake-01-in-campaign-qualified-passes` — an issue recorded in one
   campaign qualifies with placement, relevant campaigns, accepted
   contracts and derived sources.
2. `intake-02-out-of-campaign-still-qualifies` — an issue outside every
   campaign may still qualify; placement stays a later root/planning
   decision (RIPR-SPEC-0244 acceptance 4).
3. `intake-03-pr-repair-intake-overlap-visible` — an intake for
   review/repair of an existing open PR keeps the PR/claim overlap visible
   before any mutation (issue #1706 fixture 5).
4. `intake-04-narrow-single-agent-intake-passes` — a narrow one-file issue
   stays single-agent despite other active campaigns (fixture 12).
5. `intake-05-legacy-and-wrong-inputs-fail` — an unresolvable legacy
   active-goal ref, an unknown placement, an unknown relevant campaign, an
   empty contract set, an unknown issue and a wrong repository each fail
   with their exact law/route pair (fixture 6).

## Regeneration

```sh
python fixtures/work_intake_identity/author_corpus.py
```

After any regeneration, recompute `provenance.json` (sha256 of
`corpus.json`) and rerun `cargo xtask check-fixture-contracts`.

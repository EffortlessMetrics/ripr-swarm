# Selected-work root-synthesis corpora (RIPR-SPEC-0273, #1706)

Corpus for the root synthesis consumer in
`xtask/src/work_synthesis_identity.rs` (#1706, delivery slice PR B,
consuming #1636). The check validates committed `WorkSynthesisV1` rows
against a `PortfolioBasisV1` compiled from immutable captured inputs; it
selects no work, claims nothing, grants no authority and mutates nothing.
`check-fixture-contracts` recomputes the provenance digests fail-closed.

## Layout

- `corpus.json` — the five required scenarios. Each scenario names the
  captured portfolio directory it binds against (referenced, never
  duplicated: a `fixtures/work_portfolio` corpus or a RIPR-SPEC-0244
  `fixtures/work_selection_identity/captured/` variant) and one or more
  synthesis cases. A case carries a `WorkSynthesisV1` row, the expected
  outcome (`pass`/`fail`) and the exact expected law/route pairs for
  failures. Every synthesis embeds the full `SelectedWorkIdentityV1` it
  reports on, so the RIPR-SPEC-0244 selection laws run inside the
  synthesis check before the synthesis-level laws.
- `author_corpus.py` — regenerates `corpus.json` from the committed
  RIPR-SPEC-0244 selection packets (deep-copied; never mutated in place)
  and the captured snapshot identities.
- `provenance.json` — per-file SHA-256 bindings in the RIPR-SPEC-0223
  provenance convention.

## The five scenarios

1. `synthesis-01-covered-transition-synthesis-passes` — a synthesis for a
   covered selected transition passes and binds the transition, coverage
   rows, a bounded next action and derived artifacts.
2. `synthesis-02-standalone-synthesis-passes` — a standalone selection
   with zero campaign placement still supports an honest synthesis.
3. `synthesis-03-partial-coverage-contradiction-surfaced` — partial
   coverage after one campaign work item merges surfaces a named
   contradiction instead of hiding the gap (issue #1706 fixture 10
   evidence shape).
4. `synthesis-04-hidden-gap-silent-switch-and-wrong-inputs-fail` — a
   hidden uncovered row, a silent campaign switch, a mismatched reported
   transition, an evidence-free synthesis and a wrong repository each
   fail with their exact law/route pair.
5. `synthesis-05-legacy-read-only-migration-posture` — legacy refs on a
   synthesis resolve read-only as a historical campaign reference and
   emit migration posture with no authority.

## Regeneration

```sh
python fixtures/work_synthesis_identity/author_corpus.py
```

After any regeneration, recompute `provenance.json` (sha256 of
`corpus.json`) and rerun `cargo xtask check-fixture-contracts`.

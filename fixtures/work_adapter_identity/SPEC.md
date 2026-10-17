# Selected-work Codex adapter corpora (RIPR-SPEC-0277, #1706)

Corpus for the Codex adapter/delegation consumer in
`xtask/src/work_adapter_identity.rs` (#1706, delivery slice PR B,
consuming #1637 and #1649). The check validates committed
`WorkAdapterV1` delegation rows against a `PortfolioBasisV1` compiled
from immutable captured inputs; it selects no work, claims nothing,
authorizes no writer, spawns no subagent and mutates nothing. A subagent
delegation consumes the selected-work context packet plus relevant
overflow on demand and never re-reads the whole portfolio; portfolio
compilation stays in the root thread. `check-fixture-contracts`
recomputes the provenance digests fail-closed.

## Layout

- `corpus.json` — the five required scenarios. Each scenario names the
  captured portfolio directory it binds against (referenced, never
  duplicated: a `fixtures/work_portfolio` corpus or a RIPR-SPEC-0244
  `fixtures/work_selection_identity/captured/` variant) and one or more
  adapter cases. A case carries a `WorkAdapterV1` row, the expected
  outcome (`pass`/`fail`) and the exact expected law/route pairs for
  failures. Every delegation embeds the full `SelectedWorkIdentityV1` it
  delegates, so the RIPR-SPEC-0244 selection laws run inside the adapter
  check before the adapter-level laws.
- `author_corpus.py` — regenerates `corpus.json` from the committed
  RIPR-SPEC-0244 selection packets (deep-copied; never mutated in place)
  and the captured snapshot identities.
- `provenance.json` — per-file SHA-256 bindings in the RIPR-SPEC-0223
  provenance convention.

## The five scenarios

1. `adapter-01-scout-read-only-delegation-passes` — a read-only scout
   delegation over one selected issue passes.
2. `adapter-02-builder-with-prepared-claim-passes` — a builder
   delegation over a selection carrying a prepared claim and an isolated
   worktree passes.
3. `adapter-03-planner-compile-delegation-passes` — a work-planner
   delegation over an independent multi-campaign selection passes.
4. `adapter-04-resume-from-artifacts-review-passes` — a reviewer
   delegation resumed from committed artifacts only passes.
5. `adapter-05-portfolio-reread-and-wrong-inputs-fail` — a subagent
   portfolio re-read, an unprepared builder, a subagent claim-write, an
   empty derivation, a foreign repository and an unknown legacy goal each
   fail with their exact law/route pair.

## Regeneration

```sh
python fixtures/work_adapter_identity/author_corpus.py
```

After any regeneration, recompute `provenance.json` (sha256 of
`corpus.json`) and rerun `cargo xtask check-fixture-contracts`.

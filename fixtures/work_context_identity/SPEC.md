# Selected-work context-packet corpora (RIPR-SPEC-0272, #1706)

Corpus for the context/work-item packet consumer in
`xtask/src/work_context_identity.rs` (#1706, delivery slice PR B, consuming
#1632). The check validates committed `WorkContextPacketV1` rows against a
`PortfolioBasisV1` compiled from immutable captured inputs; it selects no
work, claims nothing, grants no authority and mutates nothing.
`check-fixture-contracts` recomputes the provenance digests fail-closed.

## Layout

- `corpus.json` — the six required scenarios. Each scenario names the
  captured portfolio directory it binds against (referenced, never
  duplicated: a `fixtures/work_portfolio` corpus or a RIPR-SPEC-0244
  `fixtures/work_selection_identity/captured/` variant) and one or more
  packet cases. A case carries a `WorkContextPacketV1` row, the expected
  outcome (`pass`/`fail`) and the exact expected law/route pairs for
  failures. Every packet embeds the full `SelectedWorkIdentityV1` it
  compiles context for, so the RIPR-SPEC-0244 selection laws run inside
  the context check before the packet-level laws.
- `author_corpus.py` — regenerates `corpus.json` from the committed
  RIPR-SPEC-0244 selection corpus packets (deep-copied; never mutated in
  place) and the captured snapshot identities.
- `provenance.json` — per-file SHA-256 bindings in the RIPR-SPEC-0223
  provenance convention.

## The six scenarios

1. `context-01-one-campaign-packet-passes` — a bounded
   selected-work-only packet compiles for a selection in one campaign.
2. `context-02-several-related-campaigns-packet-passes` — several related
   campaign references stay context, never permission.
3. `context-03-standalone-no-campaign-packet-passes` — a standalone issue
   with zero campaign placement still compiles a valid packet
   (RIPR-SPEC-0244 acceptance 4).
4. `context-04-pr-repair-packet-passes` — a repair packet binds the
   recorded PR, claim and worktree as relevant overflow for review/repair
   of an existing open PR instead of a new build.
5. `context-05-narrow-single-agent-packet-passes` — a narrow one-file
   issue stays single-agent despite other active campaigns.
6. `context-06-legacy-and-wrong-context-refs-fail` — an unresolvable
   legacy active-goal ref, an irrelevant carried reference, a
   selected-work-only packet with overflow, an evidence-free packet, a
   wrong repository and a stale basis head each fail with their exact
   law/route pair.

## Regeneration

```sh
python fixtures/work_context_identity/author_corpus.py
```

After any regeneration, recompute `provenance.json` (sha256 of
`corpus.json`) and rerun `cargo xtask check-fixture-contracts`.

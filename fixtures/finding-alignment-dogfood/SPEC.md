# Fixture Corpus: finding-alignment-dogfood

Spec: RIPR-SPEC-0045

## Given

RIPR has checked finding-alignment examples from real RIPR PR work. Each
example records raw finding counts, canonical item counts, evidence class,
canonical gap identity when applicable, raw finding summary, gap state,
actionability, repair or no-action route, verify command, before/after audit
context, and must-not-claim guards.

## When

```bash
cargo xtask dogfood
cargo xtask check-fixture-contracts
```

## Then

The corpus pins the Lane 1 counting model:

- raw findings are supporting evidence;
- canonical evidence items are the countable unit;
- actionable canonical gaps are user work;
- actionable cases carry a canonical gap id, repair route, and verify command;
- already-observed and internal-only items are no-action;
- no-action cases name why no user repair is selected;
- static limitations name analyzer repair routes;
- raw finding summaries and before/after audit context make fixture-backed
  movement visible without changing analyzer truth.

The corpus also pins the fixture-backed opaque report lookup burn-down case:
the supported report lookup moves to one actionable output-observer gap while
generic opaque lookup support remains a named static limitation.

## Must Not

- Infer actionability from a raw static class.
- Treat internal policy or config metadata as user test debt.
- Hide raw findings.
- Change PR/CI rendering, LSP/editor behavior, gates, public scores, source
  edits, generated tests, provider calls, or mutation execution.

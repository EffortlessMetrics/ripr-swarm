# Spec style and source-of-truth stack

ripr keeps the full source-of-truth stack, but stores durable rails in a repo-owned namespace.

## Durable ownership model

- `.ripr-spec/` = durable repo knowledge base and spec rails.
- `docs/` = human-facing explanation and contributor guidance.
- `policy/` = live enforcement ledgers; referenced where relevant.
- `plans/` = optional; only when already part of non-agent planning.

## Awareness-only external state

The following directories may exist and may be read by tools, but are not owned by this spec system:

- `.codex/`
- `.spec/`
- `.claude/`
- `.jules/`

Do not place durable proposal/spec/ADR/lane/closeout artifacts in those directories.

## Chain separation

Keep artifacts distinct so one document does not become proposal + spec + task queue + release proof + CI policy all at once:

- proposal = why
- spec = behavior contract and required evidence
- ADR = durable architecture decision
- lane tracker + implementation plan = PR-sized execution and state
- closeout = landed evidence and remaining work

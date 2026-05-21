# Rails framework footprint

This repository uses `.rails/` as the durable Rails knowledge base.

## Ownership model

- `.rails/` is the durable Rails knowledge base.
- `docs/` explains Rails to humans.
- `.codex/` is Codex execution state and is not owned by Rails.
- `.spec/` is Spec Kit / speckit state and is not owned by Rails.
- `.claude/` and `.jules/` are external agent/session spaces and are not owned by Rails.

## Durable artifact model

Rails separates durable artifacts into focused directories:

- `proposals/`: why work exists, value, alternatives, success criteria
- `specs/`: behavior contracts and required evidence
- `adr/`: durable architecture decisions
- `lanes/`: focused implementation trackers
- `templates/`: standard artifact shapes
- `closeouts/`: what landed, what proved it, what remains
- `support/`: product claim to proof mapping
- `policy/`: references to live policy ledgers
- `receipts/`: optional durable proof bundles
- `schemas/`: artifact schema definitions

## Index contract

Every Rails artifact is linked through `.rails/index.toml`.
No Rails-owned artifact path may live under `.codex/`, `.spec/`, `.claude/`, or `.jules/`.

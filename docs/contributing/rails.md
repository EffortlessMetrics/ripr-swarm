# Contributing with Rails

When adding durable proposal/spec/ADR/lane knowledge, use `.rails/`.

## Rules

- Keep durable Rails artifacts under `.rails/`.
- Do not migrate, modify, or validate `.codex/`, `.spec/`, `.claude/`, or `.jules/` as Rails-owned state.
- Keep lane sequencing focused under `.rails/lanes/<lane>/` instead of a single giant active queue.
- Link durable artifacts through `.rails/index.toml`.

## Authoring checklist

1. Add or update the artifact file under `.rails/`.
2. Add or update the `.rails/index.toml` reference.
3. Keep status, ownership, and links consistent.
4. Run proof commands for the current lane.

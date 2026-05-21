# Contributing: repo-native spec rails

When introducing or changing durable product/architecture behavior rails:

1. Add or update artifacts in `.ripr-spec/`.
2. Link artifacts through `.ripr-spec/index.toml`.
3. Keep live enforcement policies in `policy/*.toml` and reference them from `.ripr-spec/policy/ledgers.toml` when needed.
4. Keep external/tool-specific state (`.codex/`, `.spec/`, `.claude/`, `.jules/`) awareness-only.

## Minimum layout

- `.ripr-spec/README.md`
- `.ripr-spec/index.toml`
- `.ripr-spec/proposals/`
- `.ripr-spec/specs/`
- `.ripr-spec/adr/`
- `.ripr-spec/lanes/`
- `.ripr-spec/closeouts/`

## Rule

Durable artifacts must be repo-owned and tool-neutral.

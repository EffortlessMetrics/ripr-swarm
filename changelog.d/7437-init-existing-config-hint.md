<!-- section: Fixed -->
- `ripr init` no longer points an existing `ripr.toml` at the one command that
  silently resets it. The refusal now names that `--force` resets
  customizations, and points at `--dry-run` plus
  `ripr init --ci github --force` (workflow refresh that keeps the config)
  (#7437).

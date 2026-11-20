<!-- section: Fixed -->
- `ripr policy readiness`, `ripr policy waiver-aging`, `ripr pr-comments plan`,
  `ripr reports index` and `ripr reports ci-summary` refuse a `--root` that is
  missing or a file (exit 2, the same message as the other commands) instead of
  writing `target/` output relative to the current directory (#14548, #14616).

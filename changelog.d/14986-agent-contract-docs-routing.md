<!-- section: Internal / CI -->
- Qualification routing sends `.agents/skills/**`, `.claude/skills/**` and
  `docs/agent-context/**` through the Rust lanes instead of classifying them
  docs-only, so the prose assertions those tests pin cannot reach main with
  every Rust lane skipped (#14986).

<!-- section: Added -->

- `ripr init --agents claude|codex|cursor|all` writes a short, verified ripr loop
  (find a gap, write a test, verify, explicit done and stop rule, refusal
  handling) into `CLAUDE.md`, `AGENTS.md` or `.cursor/rules/ripr.mdc` as a marked
  section. It appends to your file, never rewrites your content, is idempotent,
  supports `--dry-run`, and refuses symlinks and unterminated markers; with
  `cursor`, `--mcp` also writes `.cursor/mcp.json`. New
  [agent quickstart](docs/AGENT_QUICKSTART.md), linked from `ripr --help` and the
  README, whose "Agent instructions" link no longer points users at the
  developer-only `AGENTS.md`.

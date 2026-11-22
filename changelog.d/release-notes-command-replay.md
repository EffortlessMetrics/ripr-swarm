<!-- section: Docs -->
- The 0.11 release notes and the 0.10 to 0.11 upgrade guide now name three
  upgrade steps the changelog recorded but they did not: a 0.10 Rust
  `finding_id` suppression no longer matches and
  `ripr check --suppression-policy` names the replacement id, a 0.10 receipt is
  rejected with the `ripr receipt write` command that replaces it (found by
  `--path`; `--gap` finds it only when the gap id has no path separator), and
  `ripr.toml` `[lsp] seam_diagnostics` applies again in VS Code. The notes gain
  a Security hardening section, say that MCP `ripr_refresh` analyzes instead of
  "does not analyze", and carry re-measured MCP tool-list sizes. Every command
  shown in the notes, README, quickstart and upgrade guide was replayed on a
  build of main; the README links the upgrade guide (#3921).

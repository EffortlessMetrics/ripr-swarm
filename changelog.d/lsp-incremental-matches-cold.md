<!-- section: Fixed -->
- The editor server now serves the same diagnostics from a long-running
  session as from a freshly started one. The delivery byte budget counted the
  refresh-attempt label stamped into every seam diagnostic, so a session ten
  refreshes old dropped one more diagnostic than a new session on the same
  workspace (#16004).
- A full `ripr.refresh` no longer drops the finding on the line you just
  changed in favour of repository-wide seam rows. Findings from the change are
  selected first; the seam inventory fills what is left of the budget (#16004).
- A Python, TypeScript, JavaScript or Perl file rewritten outside the editor
  (a terminal, a formatter, a coding agent) now refreshes its diagnostics even
  when it had none. Before, a clean file kept its clean result until the next
  save inside the editor; Rust files already refreshed (#16004).

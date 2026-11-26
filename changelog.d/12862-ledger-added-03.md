<!-- section: Added -->
- `ripr agent status --attempt <id>` selects exactly one repair attempt
  from the attempt store and reports one typed status (`awaiting_edit`,
  `prepared`, `finished_current`, `finished_historical`, `stale`,
  `incomparable`, `failed`, `limited`, `corrupt_or_unavailable` or
  `legacy_compatibility_only`) with one next or recovery action, read
  from repository artifacts alone, in both Markdown and `--json`. A
  missing, malformed or tampered attempt record reads
  `corrupt_or_unavailable` and never falls back to another attempt's
  receipt (#5234, #4798).
- VS Code extension: the new `ripr: Show Repair Attempt Status` command
  shows the CLI's repair-attempt status (`ripr agent status --attempt <id>
  --json`) in a status-bar item at the strength the CLI reports, asks for
  an explicit pick when several current attempts exist, remembers the
  selection per workspace root across restarts, and refuses in untrusted
  workspaces before any read (#5366, #4643).
- VS Code: `ripr: Start or Continue Repair` finishes a repair that is
  waiting for its after phase by running the shared after phase for the
  selected resumable attempt, and otherwise starts a repair as before.
  Refresh Diagnostics is titled Refresh Full Analysis (#4647, #12391).

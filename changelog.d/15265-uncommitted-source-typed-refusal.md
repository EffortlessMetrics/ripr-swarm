<!-- section: Fixed -->
- `ripr repair` over uncommitted changes outside the repair's allowed edit
  surface (usually production source) now refuses with the typed
  `start_refused_dirty_premise` code: exit 3 and, under `--json`, one
  `cli_refusal` document, like the other not-started start holds. Before, it
  exited 2 with empty `--json` output. The message is unchanged, and
  `ripr agent repair --phase before` keeps exit 2 with the new code line (#15265).

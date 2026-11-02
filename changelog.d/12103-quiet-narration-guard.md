<!-- section: Internal / CI -->
- A source-scan plus catalog runtime guard now fails when a command writes
  `ripr progress:` or `ripr: wrote` narration outside the shared CLI progress
  sink, so `RIPR_QUIET` cannot be forgotten at a new call site (#12103).

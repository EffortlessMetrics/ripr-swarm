<!-- section: Fixed -->
- TypeScript workspace discovery counts only candidate TS/JS source files
  against `RIPR_TS_MAX_WORKSPACE_FILES`, so a large non-source tree no longer
  truncates before those sources are seen. A separate 5× total-entry backstop
  (100_000 entries at the default 20_000-candidate limit) still terminates
  huge asset trees and reports the existing truncated flag. Directory entries
  are visited in sorted-name DFS order so a tiny cap still indexes `src/`
  before a `docs/` tree trips that backstop (#11984).

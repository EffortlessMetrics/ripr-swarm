<!-- section: Fixed -->
- TypeScript workspace discovery counts only admitted TS/JS source files
  against `RIPR_TS_MAX_WORKSPACE_FILES`, so a large non-source tree no longer
  truncates before those sources are seen. A separate 10× total-entry backstop
  still terminates huge asset trees and reports the existing truncated flag
  (#11984).

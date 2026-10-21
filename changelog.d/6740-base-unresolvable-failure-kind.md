<!-- section: Fixed -->
- The VS Code status bar now chooses the `ripr.baseRef` next step from a new
  stable `failure.kind` of `base_unresolvable` on `ripr/analysisStatus`
  instead of matching server message wording, so it also covers empty and
  option-like base refs. Older servers that still send `analysis_error` keep
  the old wording-based fallback (#6740).

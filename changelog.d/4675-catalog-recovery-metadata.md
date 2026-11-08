<!-- section: Added -->
- Diagnostic catalog entries now carry producer-owned recovery, actionability,
  and profile defaults (`FixInstructionState`, closed recovery kinds, visibility,
  severity source, freshness preconditions, and non-claims). Unknown codes and
  unlisted recovery kinds stay typed unavailable; client capability can hide
  presentation but cannot change catalog state. Emitted diagnostic `code`
  strings are unchanged (#4675).

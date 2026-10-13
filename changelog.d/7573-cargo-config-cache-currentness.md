<!-- section: Fixed -->
- Classified seam caches invalidate registered harness evidence after Cargo
  configuration changes, including edits with unchanged HEAD. Reverting the
  configuration preserves useful warm reuse (#7573).

<!-- section: Fixed -->
- Classified seam caches invalidate registered harness evidence after Cargo
  configuration changes, including edits with unchanged HEAD. Reverting the
  configuration preserves useful warm reuse. Cargo-compatible home discovery
  preserves classified provenance and reuse with missing/empty home environment
  values, and binds relative home configuration at the analysis root (#7573).

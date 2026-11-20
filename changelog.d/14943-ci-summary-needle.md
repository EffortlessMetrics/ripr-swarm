<!-- section: Internal / CI -->
- `cargo xtask release-readiness` checks the ci-summary first-run block for the
  artifact-relative `ripr/reports/start-here.md` path, so a summary that names
  the packet by its artifact path no longer reports a missing start-here
  needle (#14943).

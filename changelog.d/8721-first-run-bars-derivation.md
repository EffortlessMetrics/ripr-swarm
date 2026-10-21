<!-- section: Fixed -->
- The scoreboard summary on `docs/PUBLIC_PROOF.md` now discloses that the
  first-run bars are recomputed by the page from the raw walk receipts and
  that an ingested scoreboard value owns its bar once one lands. The
  renderer's derivation of the six first-run ids now applies only when the
  scoreboard receipt lacks a number (mirroring the `over_budget_workflows`
  fallback guard), so the day the nightly ingest lands first-run values the
  producer's reviewed numbers are no longer silently shadowed (#8721).

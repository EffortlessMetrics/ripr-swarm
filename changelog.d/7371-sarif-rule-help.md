<!-- section: Fixed -->
- SARIF rules now carry per-class meaning instead of restating the class
  name. Every `ripr.finding.*` and `ripr.seam.*` rule gains a real
  `fullDescription` and `help` written for a maintainer who has never
  seen ripr: what the class means, the static/advisory boundary (no test
  was executed), and the typical next step. Finding rules link
  `docs/STATIC_EXPOSURE_MODEL.md#exposure-classes` and seam rules link
  the RIPR-SPEC-0005 seam inventory contract, replacing the single
  shared severity-policy helpUri every rule used to carry (#7371).

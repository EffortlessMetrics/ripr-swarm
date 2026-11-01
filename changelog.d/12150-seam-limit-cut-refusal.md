<!-- section: Fixed -->
- `ripr outcome`, `ripr agent verify` and `ripr calibrate cargo-mutants` now refuse a
  seam-limit-cut `repo-exposure.json` (`RIPR_PILOT_SEAM_BUDGET` or
  `RIPR_REPO_EXPOSURE_SEAM_LIMIT`) by name through the shared repo-exposure cut
  predicate, so a seam missing from a capped prefix is not read as removed (#12150).

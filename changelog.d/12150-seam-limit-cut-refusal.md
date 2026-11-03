<!-- section: Fixed -->
- `ripr outcome`, `ripr agent verify` and `ripr calibrate cargo-mutants` now refuse a
  seam-limit-cut `repo-exposure.json` (`RIPR_PILOT_SEAM_BUDGET` or
  `RIPR_REPO_EXPOSURE_SEAM_LIMIT`) by name through the shared repo-exposure cut
  predicate, so a seam missing from a capped prefix is not read as removed (#12150).
- `ripr pilot` after a seam-limited run now prefixes its suggested before and after
  `check` snapshot commands with `RIPR_REPO_EXPOSURE_SEAM_LIMIT=0` (a scoped
  `$env:` set in the PowerShell form), so on a repository above the default
  inventory cap the advertised `ripr outcome` comparison is not itself cut (#12150).

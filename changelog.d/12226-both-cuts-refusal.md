<!-- section: Fixed -->
- A pilot `repo-exposure.json` cut both by the byte budget and by a seam limit (the default above 2,000
  seams) is now refused once by `ripr outcome`, `ripr agent verify` and `ripr calibrate cargo-mutants`,
  naming every control (`RIPR_PILOT_REPO_EXPOSURE_BYTES` and `RIPR_PILOT_SEAM_BUDGET` or
  `RIPR_REPO_EXPOSURE_SEAM_LIMIT`) in one repair route, instead of a second refusal after the first
  repair (#12226).

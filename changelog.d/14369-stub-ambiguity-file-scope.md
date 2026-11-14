<!-- section: Fixed -->
- `ripr check`: a finding whose line holds several stub seams no longer classifies the whole workspace to pick one. On ripr-swarm a two-file diff check drops from about 18 minutes to about 3 (#14369).
- `ripr agent stub --seam-id`: a seam past the 10,000-seam inventory cap of a large workspace is found instead of reported as "not found", and the lookup classifies that seam's file instead of the whole workspace. On ripr-swarm a changed seam's stub takes about 2.5 minutes where it failed after 10 (#14369).

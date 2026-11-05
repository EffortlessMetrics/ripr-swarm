<!-- section: Changed -->
- `ripr evidence-health` now shows the same stderr stage lines and heartbeats as `ripr check` while it walks the workspace, so a long run on a very large source file no longer looks like a hang; `--quiet` (or `RIPR_QUIET`) silences them (#13055).

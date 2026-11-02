<!-- section: Internal / CI -->
- The isolated policy-facade check now prints a start and finish line (with elapsed seconds) for each Cargo command that `check-file-policy` runs, so a timeout on a slow Windows runner names the command it was inside (#12510).

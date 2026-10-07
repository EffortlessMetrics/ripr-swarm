<!-- section: Fixed -->
- `ripr pilot` ranks seams on lines changed by the current change (the base
  against the working tree when it has uncommitted tracked changes, else
  `<base>...HEAD`) ahead of the repo-wide order, and its terminal,
  `pilot-summary.md` and `pilot-summary.json` say whether the top
  recommendation is part of that change. When the change has no ranked seam,
  pilot says the recommendation is elsewhere in the repo and points to
  `ripr check` for the change itself, with `--worktree` when the change is
  uncommitted (plain `ripr check` reads committed history only). The
  "Inspected" block names the scope: `change-first (Rust seams ...)` with a
  change, otherwise `whole repository` (with a short reason when the change
  could not be loaded); a Python preview repair card is still chosen from the
  committed diff. With no change, or when the diff cannot be loaded, the
  ranking is unchanged; `pilot-summary.json` adds a `current_change` object
  whose `state` keeps `no_change` and `unavailable` apart. `RIPR_GIT_TIMEOUT`
  bounds these git calls as it does for `ripr check`; an invalid value fails
  pilot before analysis (#1169).

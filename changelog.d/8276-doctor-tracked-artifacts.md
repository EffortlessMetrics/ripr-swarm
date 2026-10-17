<!-- section: Fixed -->
- `ripr doctor` now warns when git already tracks reports under `target/ripr`
  even though an ignore rule covers them, names `git rm --cached -r --
  target/ripr`, and reports the state as `cache.tracked_artifacts` in
  `doctor --json` (#8276).

<!-- section: Added -->
- `ripr doctor` now warns when git already tracks files under `target/ripr`,
  even if an ignore rule covers the directory, and names
  `git rm --cached -r -- target/ripr` as the fix. `doctor --json` carries the
  separate `cache.artifact_tracked` field (#8276).

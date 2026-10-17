<!-- section: Added -->
- `ripr doctor` now warns when git already tracks files under `target/ripr`,
  which a `.gitignore` entry cannot stop `git add -A` from refreshing, and
  names `git rm --cached -r -- target/ripr`; `doctor --json` carries the typed
  `cache.artifact_tracking` field (#8276).

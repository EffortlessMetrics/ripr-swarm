<!-- section: Added -->
- `ripr doctor` now warns when git already tracks files under `target/ripr`,
  even when `.gitignore` covers it: an ignore rule does not untrack reports
  committed earlier, so `git add -A` kept refreshing them and `ripr check`
  kept diffing them. The warning names `git rm --cached -r -- target/ripr`.
  `doctor --json` carries it as the new `cache.artifact_tracking` field
  (`untracked`, `not_checked`, or `tracked` with the warning), separate from
  `cache.artifact_ignore` (#8276).

<!-- section: Added -->
- `ripr doctor` now warns when git already tracks files under `target/ripr`, even if an ignore rule covers it, and names `git rm --cached -r -- target/ripr`. `doctor --json` carries the verdict as `cache.artifact_tracked`, separate from `cache.artifact_ignore` (#8276).

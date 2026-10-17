<!-- section: Added -->
- `ripr doctor` warns when git already tracks files under `target/ripr`, even when `/target/ripr/` is ignored, and names `git rm --cached -r -- target/ripr` as the fix. `doctor --json` carries the verdict as `cache.artifact_tracking`, separate from `cache.artifact_ignore` (#8276).

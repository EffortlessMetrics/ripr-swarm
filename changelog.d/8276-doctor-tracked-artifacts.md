<!-- section: Added -->
- `ripr doctor` now warns when ripr's own artifacts under `target/ripr` are already tracked in the git index, even when a `.gitignore` rule covers them, and names `git rm --cached -r -- target/ripr`. `doctor --json` carries it as the new `cache.artifact_tracking` field (#8276).

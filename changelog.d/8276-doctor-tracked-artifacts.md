<!-- section: Fixed -->
- `ripr doctor` warns when ripr artifacts under `target/ripr` are still tracked in the git index, even when `/target/ripr/` is already ignored, and names `git rm --cached -r -- target/ripr`; `doctor --json` carries the separate `cache.tracked_artifacts` field (#8276).

<!-- section: Fixed -->
- `ripr doctor` warns when files under `target/ripr` are already tracked in the
  git index (even with a covering `.gitignore` rule) and names
  `git rm --cached -r -- target/ripr`; `doctor --json` carries it as the new
  `cache.artifact_tracking` field (#8276).

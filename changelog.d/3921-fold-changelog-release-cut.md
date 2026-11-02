<!-- section: Internal / CI -->
- `cargo xtask fold-changelog --write --release-cut` is the one cut-day command: it folds every fragment (it cannot be combined with `--limit` or `--keep-fragments`), deletes them, warns when the PR would exceed the 160-file admission cap, and then requires `check-changelog-fragments --release-cut` to pass (#3921).

<!-- section: Internal / CI -->
- `cargo xtask fold-changelog` runs the whole `check-changelog-fragments` check before it folds, on dry runs and on both `--write` routes, and refuses with `CHANGELOG.md` and every fragment untouched when it fails; `--write --release-cut` used to report such a failure only after rewriting both (#15606).

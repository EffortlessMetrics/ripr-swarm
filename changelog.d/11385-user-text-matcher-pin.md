<!-- section: Internal / CI -->
- `check-user-text` pins its own matcher against the merge-base: under CI it fails a PR whose matcher no longer detects a banned term the base listed, and a run that is not a pull request or merge group (a push to `main`) now fails on a golden over its per-file ceiling instead of warning, so a gate weakened in a PR is visible on `main` too (#11385).

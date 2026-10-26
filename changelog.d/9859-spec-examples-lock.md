<!-- section: Internal / CI -->
- The verdict corpora protect their covered spec examples through a sorted `spec-examples.lock` (one example per line) instead of a single `floor` count in `spec-coverage.toml`, so corpus PRs that cover different examples no longer conflict on one line and a swapped example is caught (#9859).

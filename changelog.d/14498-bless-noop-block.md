<!-- section: Internal / CI -->
- `cargo xtask goldens bless` no longer appends a `Pending` block to
  `expected/CHANGELOG.md` when every expected file already matches the run
  output: the bless compares each surface first, copies and names only the ones
  that changed, and reports "No change" when none did (#14498).

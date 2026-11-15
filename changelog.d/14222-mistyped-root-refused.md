<!-- section: Fixed -->
- `reports ci-packet`, `feedback export` and `feedback record` now refuse a
  `--root` that does not exist or is not a directory with exit 2, through the
  same check as `ripr check`. `reports ci-packet` and `feedback export` used to
  create the mistyped path as a new directory tree and exit 0 (#14222).

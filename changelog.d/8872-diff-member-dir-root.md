<!-- section: Fixed -->
- `ripr diff` run from a Cargo workspace member directory now resolves the
  workspace root through the same implicit-root owner as `ripr check` (same
  stderr disclosure; `--root .` opts out) (#8872).

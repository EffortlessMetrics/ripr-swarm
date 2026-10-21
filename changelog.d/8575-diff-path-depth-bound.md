<!-- section: Fixed -->
- `ripr check --diff` (and `explain`, `context`) no longer hang or run out of
  memory on a diff whose path header has tens of thousands of components. A
  path deeper than 256 components is refused up front with
  `diff_path_too_deep` and exit 2 (#8575).

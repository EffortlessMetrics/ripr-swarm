<!-- section: Fixed -->
- `policy preview-promote` now refuses a missing or file `--root` with the
  shared `policy preview-promote root <path> is not a directory` message
  instead of exiting 0 and writing its default reports under the current
  directory (#14624).
